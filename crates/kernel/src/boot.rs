//! Kernel entrypoint and everything related to boot into the kernel

use crate::allocator::{self, order_for_size, size_for_order, PAGE_SIZE};
use crate::{
    drivers, hart,
    page::{self, Flags, KernelPageTable, PageSize, PhysAddr, VirtAddr},
    pmem, symbols, trap, unit, StaticCell,
};
use alloc::boxed::Box;
use core::slice;
use devicetree::DeviceTree;
use riscv::csr::satp;

mod harts;

//static PAGE_TABLE: StaticCell<KernelPageTable> = StaticCell::new(KernelPageTable::new());

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

    let mut table = KernelPageTable::new();
    table
        .map(
            0x1000_0000.into(),
            0x1020_0000.into(),
            PageSize::Megapage,
            Flags::READ,
        )
        .unwrap();
    log::debug!("{:?}", table.debug());

    sbi::system::shutdown();

    // get access to the global page table
    let table: &mut KernelPageTable = todo!();

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

    //map_section(symbols::text_range(), Flags::READ | Flags::EXEC);
    //map_section(symbols::rodata_range(), Flags::READ);
    //map_section(symbols::data_range(), Flags::READ | Flags::WRITE);
    //map_section(symbols::tdata_range(), Flags::READ | Flags::WRITE);
    //map_section(symbols::bss_range(), Flags::READ | Flags::WRITE);
    //map_section(symbols::stack_range(), Flags::READ | Flags::WRITE);
    // FIXME: Link and map sections properly!
    map_section(
        symbols::kernel_range(),
        Flags::READ | Flags::WRITE | Flags::EXEC,
    );

    // allocate the stack for this hart
    let (_, stack) = alloc_kernel_stack(table, hart_id as u64);

    // enable paging
    satp::write(satp::Satp {
        asid: 0,
        mode: satp::Mode::Sv39,
        root_table: table as *const _ as u64,
    });
    riscv::asm::sfence(None, None);

    // we need to convert the devicetree to use virtual memory
    let fdt = DeviceTree::from_ptr(page::phys2virt(fdt.as_ptr()).as_ptr()).unwrap();

    // jump to rust code using the trampoline
    entry_trampoline(
        hart_id,
        page::phys2virt(&fdt as *const _).as_ptr(),
        stack.into(),
        rust_trampoline as usize,
    )
}

/// Trampoline to jump enable paging and transition to new stack.
#[naked]
unsafe extern "C" fn entry_trampoline(
    _hart_id: usize,
    _fdt: *const DeviceTree<'_>,
    _new_stack: usize,
    _dst: usize,
) -> ! {
    #[rustfmt::skip]
    asm!("
        # This trampoline code is responsible for switchting to a new stack, that
        # is located in virtual memory, by copying the old stack into the new one.
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
        jr a3
    ",
    options(noreturn));
}

/// Wrapper around the `main` call to avoid marking `main` as `extern "C"`.
///
/// This function also brings up the other harts.
unsafe extern "C" fn rust_trampoline(hart_id: usize, fdt: &'static DeviceTree<'static>) -> ! {
    // install the interrupt handler
    trap::install_handler();

    // create the devicemanager
    let devices = Box::leak(Box::new(drivers::DeviceManager::from_devicetree(fdt)));

    // initialize hart local storage and hart context
    hart::init_hart_context(hart_id as u64, true, devices, fdt).unwrap();
    hart::init_hart_local_storage().unwrap();

    // boot up the other harts
    let table: &mut KernelPageTable = todo!();
    harts::boot_all_harts(hart_id, fdt, table);

    // jump into safe rust code
    crate::main(fdt)
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

            # Load the link start address
            li t0, {}
            lla t1, __kernel_start
            sub t2, t1, t0

            lla t0, __rel_dyn_start
            lla t1, __rel_dyn_end

            beq t0, t1, 4f
            j 2f

        1:
            ld t5, -16(t0)
            li t3, 3
            bne t3, t5, 3f
            ld t3, -24(t0) 
            ld t5, -8(t0) 
            add t5, t5, t2
            add t3, t3, t2
            sd t5, 0(t3)
            j 2f

        2:
            addi t0, t0, 8 * 3
            ble t0, t1, 1b
            j 4f

        3:
            # An error occurred. For now we are just looping
            j 3b

        4:
            # Zero bss section
            la t0, __bss_start
            la t1, __bss_end

        _zero_bss:
            bgeu t0, t1, _zero_bss_done
            sd zero, (t0)
            addi t0, t0, 8
            j _zero_bss

        _zero_bss_done:

            # Jump into rust code
            la sp, __stack_end
            j _before_main",
        const crate::symbols::LINK_START,
        options(noreturn)
    )
}
