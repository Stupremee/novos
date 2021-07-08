//! Kernel entrypoint and everything related to boot into the kernel

mod harts;
mod reloc;

use crate::{
    allocator::{self, order_for_size, size_for_order, PAGE_SIZE},
    hart,
    memmap::{self, KERNEL_PHYS_MEM_BASE, KERNEL_STACK_BASE, KERNEL_STACK_SIZE},
    page::{Flags, KernelPageTable, PageSize, PhysAddr, VirtAddr},
    pmem, symbols, trap, unit,
};
use alloc::boxed::Box;
use core::slice;
use devicetree::DeviceTree;
use riscv::sync::Mutex;

#[doc(hidden)]
pub(crate) static PAGE_TABLE: Mutex<Option<KernelPageTable>> = Mutex::new(None);

struct SbiLogger;
impl log::Logger for SbiLogger {
    fn write_str(&self, x: &str) -> core::fmt::Result {
        let _ = x.chars().try_for_each(sbi::legacy::put_char);
        Ok(())
    }
}

/// Allocates the stack for a hart, with the given id and returns the end address
/// of the new stack.
///
/// Returns both, the physical and virtual address to the end of the stack.
pub(self) fn alloc_kernel_stack(table: &mut KernelPageTable, id: u64) -> (PhysAddr, VirtAddr) {
    // calculate the start address for hart `id`s stack
    let start = KERNEL_STACK_BASE + id as usize * KERNEL_STACK_SIZE;

    // allocate the backing physmem
    let stack = pmem::alloc_order(allocator::order_for_size(KERNEL_STACK_SIZE))
        .unwrap()
        .as_ptr();

    // allocate the new stack
    for off in (0..KERNEL_STACK_SIZE).step_by(PAGE_SIZE) {
        table
            .map(
                unsafe { stack.add(off) }.into(),
                (start + off).into(),
                PageSize::Kilopage,
                Flags::READ | Flags::WRITE | Flags::ACCESSED | Flags::DIRTY,
            )
            .unwrap();
    }

    (
        PhysAddr::from(stack as usize + KERNEL_STACK_SIZE),
        VirtAddr::from(start + KERNEL_STACK_SIZE),
    )
}

/// The code that sets up memory stuff,
/// allocates a new stack and then runs the real main function.
#[no_mangle]
unsafe extern "C" fn _before_main(hart_id: usize, fdt: *const u8) -> ! {
    let fdt = DeviceTree::from_ptr(fdt).unwrap();

    // if the underyling sbi implementation supports `put_char`, use it as a
    // temporary logger
    if sbi::base::probe_ext(0x01).unwrap_or(false) {
        log::init_log(SbiLogger).map_err(|_| ()).unwrap();
    }

    // initialize the physmem allocator
    pmem::init(&fdt).unwrap();

    // get access to the global page table
    let mut table_lock = PAGE_TABLE.lock();
    let table = table_lock.get_or_insert_with(|| KernelPageTable::new());

    // copy the devicetree to a newly allocated physical page
    let fdt_order = order_for_size(fdt.total_size() as usize);
    let new_fdt = pmem::alloc_order(fdt_order).unwrap();
    assert_ne!(
        (new_fdt.as_ptr() as usize) >> 12,
        (&fdt as *const _ as usize) >> 12,
    );

    let new_fdt = slice::from_raw_parts_mut(new_fdt.as_ptr(), size_for_order(fdt_order));
    let fdt: DeviceTree<'static> = fdt.copy_to_slice(new_fdt);

    // get all available physical memory from the devicetree and map it
    // at the physmem base
    let phys_mem = fdt.memory().regions().next().unwrap();
    for page in (phys_mem.start()..phys_mem.end()).step_by(2 * unit::MIB) {
        let vaddr = page + KERNEL_PHYS_MEM_BASE;
        table
            .map(
                page.into(),
                vaddr.into(),
                PageSize::Megapage,
                Flags::READ | Flags::WRITE | Flags::ACCESSED | Flags::DIRTY,
            )
            .unwrap();
    }

    // get the base address of the real memory location where the kernel currently is
    let (base, _) = symbols::kernel_range();
    let base = base as usize;

    // map the kernel sections
    let mut map_section = |(o_start, o_end): (*mut u8, *mut u8), perm: Flags| {
        // only get the offset of this section
        let (start, end) = (o_start as usize - base, o_end as usize - base);

        for section_off in (start as usize..end as usize).step_by(PAGE_SIZE) {
            // map the physical page to the same offset in the higher half of address space
            let phys = base + section_off;
            let virt = memmap::HIGHER_HALF_START + section_off;

            table
                .map(
                    phys.into(),
                    virt.into(),
                    PageSize::Kilopage,
                    perm | Flags::ACCESSED | Flags::DIRTY,
                )
                .unwrap();
        }
    };

    map_section(symbols::text_range(), Flags::READ | Flags::EXEC);
    map_section(symbols::rodata_range(), Flags::READ);
    map_section(symbols::data_range(), Flags::READ | Flags::WRITE);
    map_section(symbols::tdata_range(), Flags::READ | Flags::WRITE);
    map_section(symbols::bss_range(), Flags::READ | Flags::WRITE);
    map_section(symbols::stack_range(), Flags::READ | Flags::WRITE);

    // allocate the stack for this hart
    let (phys_stack, virt_stack) = alloc_kernel_stack(table, hart_id as u64);

    // calculate the address for the function to trampoline into
    let real_addr = rust_trampoline as usize;
    let off = real_addr - base;
    let virt_addr = memmap::HIGHER_HALF_START + off;

    let satp = table.satp();

    // before enabling paging, we need to convert the global page table to use virtual addresses
    replace_with::replace_with(
        table,
        || sbi::system::fail_shutdown(),
        |me| {
            // get the raw parts from the current pagetable
            let (entries, (ptr, len, cap)) = me.into_raw_parts();

            // convert the pointers to virtual addresses
            let entries = entries.cast::<u8>().add(KERNEL_PHYS_MEM_BASE).cast();
            let ptr = ptr.cast::<u8>().add(KERNEL_PHYS_MEM_BASE).cast();

            // create the new, converted page table
            let table = KernelPageTable::from_raw_parts(
                Box::from_raw_in(entries, pmem::GlobalPhysicalAllocator),
                alloc::vec::Vec::from_raw_parts_in(ptr, len, cap, pmem::GlobalPhysicalAllocator),
            );
            table
        },
    );

    // drop the page table to unlock the lock
    drop(table_lock);

    // set the physical memory offset
    memmap::set_phymem_offset(KERNEL_PHYS_MEM_BASE);

    // relocate kernel before jumping to virtual memory
    assert_eq!(reloc::relocate(memmap::HIGHER_HALF_START), 0);

    // jump to rust code using the trampoline
    entry_trampoline(
        hart_id,
        fdt.as_ptr().add(KERNEL_PHYS_MEM_BASE),
        satp.as_bits(),
        virt_stack.into(),
        virt_addr,
    )
}

/// Trampoline to jump enable paging and transition to new stack.
#[naked]
unsafe extern "C" fn entry_trampoline(
    _hart_id: usize,
    _fdt: *const u8,
    _satp: usize,
    _virt_stack: usize,
    _dst: usize,
) -> ! {
    #[rustfmt::skip]
    asm!("
         # Load new stack
         mv sp, a3

         # Prepare the stvec register, so after enabling paging, we trap into out target function
         csrw stvec, a4

         # Enable paging. This will trap because this code here is not mapped anymore.
         csrw satp, a2
         sfence.vma
         nop

    ",
    options(noreturn));
}

/// Wrapper around the `main` call to avoid marking `main` as `extern "C"`.
///
/// This function also brings up the other harts.
#[repr(align(4))]
unsafe extern "C" fn rust_trampoline(hart_id: usize, fdt: *const u8, satp: u64) -> ! {
    let fdt = DeviceTree::from_ptr(fdt).unwrap();
    hart::init_hart_context(hart_id as u64, hart_id as u64, fdt).unwrap();

    // install the interrupt handler
    trap::install_handler();

    // override the logger so it will use virtual addresses too
    log::override_log(SbiLogger).map_err(|_| ()).unwrap();

    // initialize hart local storage and hart context
    hart::init_hart_local_storage().unwrap();

    // FIXME: Currently broken
    // boot up the other harts
    // harts::boot_all_harts(hart_id, fdt, satp);

    // jump into safe rust code
    crate::main(&fdt)
}

/// The entrypoint for the whole kernel.
///
/// `a0` = hart id
/// `a1` = pointer to device tree
#[naked]
#[no_mangle]
#[link_section = ".text.init"]
unsafe extern "C" fn _boot() -> ! {
    asm!(
        "
            # Load the global pointer into
            # the `gp` register
        .option push
        .option norelax
            lla gp, __global_pointer$
        .option pop

            # Disable interrupts
            csrw sie, zero
            csrci sstatus, 2

            # Relocate the kernel to the base address where we are currently linked
            lla sp, __stack_end

            # Save the arguments we need for later
            addi sp, sp, -16
            sd a0, 0(sp)
            sd a1, 8(sp)

            lla a0, __kernel_start
            call {reloc}

            # If relocate returned 0, we successfully relocated
            beqz a0, 2f

        1:
            # An error occurred. For now we are just looping
            j 1b

        2:
            # Zero bss section
            lla t0, __bss_start
            lla t1, __bss_end

        _zero_bss:
            bgeu t0, t1, _zero_bss_done
            sd zero, (t0)
            addi t0, t0, 8
            j _zero_bss

        _zero_bss_done:

            # Load arguments from stack
            ld a0, 0(sp)
            ld a1, 8(sp)
            addi sp, sp, 16

            # Jump into rust code
            j _before_main",
        reloc = sym reloc::relocate,
        options(noreturn)
    )
}
