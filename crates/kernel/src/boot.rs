//! Kernel entrypoint and everything related to boot into the kernel

mod harts;
mod reloc;

use crate::allocator::{self, order_for_size, size_for_order, PAGE_SIZE};
use crate::{
    hart,
    page::{Flags, KernelPageTable, PageSize, PhysAddr, VirtAddr},
    pmem, symbols, trap, unit,
};
use alloc::boxed::Box;
use core::slice;
use devicetree::DeviceTree;
use riscv::sync::Mutex;

#[doc(hidden)]
pub(crate) static PAGE_TABLE: Mutex<Option<KernelPageTable>> = Mutex::new(None);

/// The base virtual addresses where the stack for every hart is located.
pub const KERNEL_STACK_BASE: usize = 0x001D_DD00_0000;

/// The stack size for each hart.
pub const KERNEL_STACK_SIZE: usize = 1024 * 1024;

/// The virtual address at which the physical memory is mapped in, such that adding
/// this constant to any "real" physaddr returns the new physaddr which can be used if
/// paging is activaed.
pub const KERNEL_PHYS_MEM_BASE: usize = 0x001F_FF00_0000;

/// The base virtual address where the allocator will start allocating virtual memory.
pub const KERNEL_VMEM_ALLOC_BASE: usize = 0x0000_AA00_0000;

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

    // map the kernel sections
    let mut map_section = |(start, end): (*mut u8, *mut u8), perm: Flags| {
        for page in (start as usize..end as usize).step_by(PAGE_SIZE) {
            table
                .map(
                    page.into(),
                    page.into(),
                    PageSize::Kilopage,
                    perm | Flags::ACCESSED | Flags::DIRTY,
                )
                .unwrap();
        }
    };

    assert_eq!(reloc::relocate(0x8020_0000), 0);

    //map_section(
    //(0x8000_0000 as *mut u8, 0x8020_0000 as *mut u8),
    //Flags::READ | Flags::EXEC,
    //);
    map_section(symbols::text_range(), Flags::READ | Flags::EXEC);
    map_section(symbols::rodata_range(), Flags::READ);
    map_section(symbols::data_range(), Flags::READ | Flags::WRITE);
    map_section(symbols::tdata_range(), Flags::READ | Flags::WRITE);
    map_section(symbols::bss_range(), Flags::READ | Flags::WRITE);
    map_section(symbols::stack_range(), Flags::READ | Flags::WRITE);
    // FIXME: Link and map sections properly!
    //map_section(
    //symbols::kernel_range(),
    //Flags::READ | Flags::WRITE | Flags::EXEC,
    //);

    // allocate the stack for this hart
    let (_, stack) = alloc_kernel_stack(table, hart_id as u64);

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

    // jump to rust code using the trampoline
    entry_trampoline(
        hart_id,
        fdt.as_ptr().add(KERNEL_PHYS_MEM_BASE),
        stack.into(),
        satp.as_bits(),
    )
}

/// Trampoline to jump enable paging and transition to new stack.
#[naked]
unsafe extern "C" fn entry_trampoline(
    _hart_id: usize,
    _fdt: *const u8,
    _new_stack: usize,
    _satp: usize,
) -> ! {
    #[rustfmt::skip]
    asm!("
        # This trampoline code is responsible for switchting to a new stack, that
        # is located in virtual memory, by copying the old stack into the new one.
        csrw satp, a3
        sfence.vma

        mv t0, sp
        mv sp, a2
        mv t1, a2
        lla t2, __stack_end

    copy_stack:
        bleu t2, t0, copy_stack_done

        addi t2, t2, -8
        addi t1, t1, -8
        addi sp, sp, -8

        ld t3, (t2)
        sd t3, (t1)
        
        j copy_stack

        # Jump into rust code again
    copy_stack_done:
        csrr a2, satp
        j {dst}
    ",
    dst = sym rust_trampoline,
    options(noreturn));
}

/// Wrapper around the `main` call to avoid marking `main` as `extern "C"`.
///
/// This function also brings up the other harts.
unsafe extern "C" fn rust_trampoline(hart_id: usize, fdt: *const u8, satp: u64) -> ! {
    let fdt = DeviceTree::from_ptr(fdt).unwrap();

    // install the interrupt handler
    trap::install_handler();

    // initialize hart local storage and hart context
    hart::init_hart_context(hart_id as u64, hart_id as u64, fdt).unwrap();
    hart::init_hart_local_storage().unwrap();

    // boot up the other harts
    harts::boot_all_harts(hart_id, fdt, satp);

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
