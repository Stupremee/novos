//! Kernel entrypoint and everything related to boot into the kernel

use crate::allocator::{order_for_size, size_for_order, PAGE_SIZE};
use crate::drivers::{ns16550a, DeviceTreeDriver};
use crate::{
    hart,
    page::{self, PageSize, PageTable, Perm, PhysAddr, VirtAddr},
    pmem, unit, StaticCell,
};
use core::{mem, slice};
use devicetree::DeviceTree;
use riscv::{csr::satp, symbols};

static PAGE_TABLE: StaticCell<page::sv39::Table> = StaticCell::new(page::sv39::Table::new());

/// The base virtual addresses where the stacks for every hart are located.
pub const KERNEL_STACK_BASE: usize = 0x000A_AAA0_0000;

/// The stack size for each hart.
pub const KERNEL_STACK_SIZE: usize = 1024 * 1024;

/// The virtual address at which the physical memory is mapped in, such that adding
/// this constant to any "real" physaddr returns the new physaddr which can be used if
/// paging is activaed.
pub const KERNEL_PHYS_MEM_BASE: usize = 0x001F_FF00_0000;

/// The maximum number of harts that will try to be started.
pub const HART_COUNT: u64 = 4;

/// Structure that is used to pass data to the harts that are started
/// from the boot hart.
#[repr(C)]
struct HartArgs {
    id: u64,
    stack: *const u8,
    satp: usize,
}

/// Allocates the stack for a hart, with the given id and returns the end address
/// of the new stack.
///
/// Returns both, the physical and virtual address to the end of the stack.
fn alloc_kernel_stack(table: &mut page::sv39::Table, id: u64) -> (PhysAddr, VirtAddr) {
    // calculate the start address for hart `id`s stack
    let start = KERNEL_STACK_BASE + (id as usize * KERNEL_STACK_SIZE);

    // TODO: map guard pages around the stack
    // allocate the new stack
    table
        .map_alloc(
            start.into(),
            KERNEL_STACK_SIZE / PAGE_SIZE,
            PageSize::Kilopage,
            Perm::READ | Perm::WRITE,
        )
        .unwrap();

    // get the physical and virtual address
    let vaddr = start + KERNEL_STACK_SIZE;
    let paddr: usize = table.translate(start.into()).unwrap().0.into();

    (PhysAddr::from(paddr + KERNEL_STACK_SIZE), vaddr.into())
}

/// The code that sets up memory stuff,
/// allocates a new stack and then runs the real main function.
#[no_mangle]
unsafe extern "C" fn _before_main(hart_id: usize, fdt: *const u8) -> ! {
    let fdt = DeviceTree::from_ptr(fdt).unwrap();

    // try to find a uart device, and then set it as the global logger
    if let Some(uart) = ns16550a::Device::from_chosen(fdt.chosen()) {
        match log::init_log(uart) {
            Ok(_) => log::info!(
                "{} the global logging system using UART.",
                "Initialized".green()
            ),
            // special case, we will instantly shutdown if we failed to initialize the logger
            // this is a pseudo panic since panic wont print anything
            Err(mut uart) => {
                use core::fmt::Write;

                // FIXME: Use colors here
                write!(
                    uart,
                    "Failed to initialize the global logger. Shutting down..."
                )
                .unwrap();
                sbi::system::fail_shutdown();
            }
        }
    }

    // initialize the physmem allocator
    pmem::init(&fdt).unwrap();

    // copy the devicetree to a newly allocated physical page
    let fdt_order = order_for_size(fdt.total_size() as usize);
    let new_fdt = pmem::alloc_order(fdt_order).unwrap();
    assert_ne!(
        (new_fdt.as_ptr() as usize) >> 12,
        (&fdt as *const _ as usize) >> 12,
    );

    let new_fdt = slice::from_raw_parts_mut(new_fdt.as_ptr(), size_for_order(fdt_order));
    let fdt: DeviceTree<'static> = fdt.copy_to_slice(new_fdt);

    // initialize hart local storage and hart context
    hart::init_hart_context(0).unwrap();

    // get access to the global page table
    let table = &mut *PAGE_TABLE.get();

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
                Perm::READ | Perm::WRITE,
            )
            .unwrap();
    }

    // map the kernel sections
    let mut map_section = |(start, end): (*mut u8, *mut u8), perm: Perm| {
        for page in (start as usize..end as usize).step_by(PAGE_SIZE) {
            table
                .map(page.into(), page.into(), PageSize::Kilopage, perm)
                .unwrap();
        }
        log::debug!("Mapped kernel section {:x?}..{:x?} | {}", start, end, perm);
    };

    map_section(symbols::text_range(), Perm::READ | Perm::EXEC);
    map_section(symbols::rodata_range(), Perm::READ);
    map_section(symbols::data_range(), Perm::READ | Perm::WRITE);
    map_section(symbols::tdata_range(), Perm::READ | Perm::WRITE);
    map_section(symbols::bss_range(), Perm::READ | Perm::WRITE);
    map_section(symbols::stack_range(), Perm::READ | Perm::WRITE);

    // allocate the stack for this hart
    let (_, stack) = alloc_kernel_stack(table, 0);

    // map the whole MMIO space (<0x8000_0000)
    for addr in (0..0x8000_0000).step_by(unit::GIB) {
        table
            .map(
                addr.into(),
                addr.into(),
                PageSize::Gigapage,
                Perm::READ | Perm::WRITE,
            )
            .unwrap();
    }

    // enable paging
    satp::write(satp::Satp {
        asid: 0,
        mode: satp::Mode::Sv39,
        root_table: table as *const _ as u64,
    });
    riscv::asm::sfence(None, None);

    // jump to rust code using the trampoline
    entry_trampoline(hart_id, &fdt, stack.into(), rust_trampoline as usize)
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
        mv t0, sp
        mv sp, a2
        mv t1, a2
        la t2, __stack_end

    copy_stack:
        bleu t2, t0, copy_stack_done

        addi t2, t2, -8
        addi t1, t1, -8
        addi sp, sp, -8

        ld t3, (t2)
        sd t3, (t1)
        
        j copy_stack

        // Jump into rust code again
    copy_stack_done:
        jr a3
    ",
    options(noreturn));
}

/// Wrapper around the `main` call to avoid marking `main` as `extern "C"`.
///
/// This function also brings up the other harts.
unsafe extern "C" fn rust_trampoline(hart_id: usize, fdt: &DeviceTree<'_>, satp: usize) -> ! {
    // get access to the global page table, because we need it to map
    // the stack for every hart
    let mut table = page::root();

    // get a new stack for the hart that will be spawned
    let (phys_stack, virt_stack) = alloc_kernel_stack(&mut *table, 1);
    log::debug!(
        "{:x?} - {:x?}",
        usize::from(phys_stack),
        usize::from(virt_stack)
    );

    // construct the arguments that the new hart will receive
    let args = HartArgs {
        id: 1337,
        stack: virt_stack.as_ptr(),
        satp,
    };

    // write them to the top of the new stack
    let args_ptr = usize::from(phys_stack) - mem::size_of::<HartArgs>();
    page::phys2virt(args_ptr).as_ptr::<HartArgs>().write(args);

    sbi::hsm::start(
        hart_id.wrapping_sub(1).min(3),
        hart_entry as usize,
        phys_stack.into(),
    )
    .unwrap();

    crate::main(fdt);
    sbi::system::shutdown()
}

#[naked]
unsafe extern "C" fn hart_entry(_hart_id: usize, _stack: usize) -> ! {
    asm!("
        ld sp, -8(a1)
        ld t0, -16(a1)
        li t1, 24
        sub sp, sp, t1
        mv a0, sp

        csrw satp, t0
        j {entry}
    ",
        entry = sym rust_hart_entry,
        options(noreturn)
    )
}

#[no_mangle]
unsafe extern "C" fn rust_hart_entry(args: &'static HartArgs) -> ! {
    //hart::init_hart_context(args.id).unwrap();
    //log::debug!("hello from hart {}", args.id);
    loop {}
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
            // Load the global pointer into
            // the `gp` register
        .option push
        .option norelax
            la gp, __global_pointer$
        .option pop

            // Disable interrupts
            csrw sie, zero
            csrci sstatus, 2

            // Zero bss section
            la t0, __bss_start
            la t1, __bss_end

        zero_bss:
            bgeu t0, t1, zero_bss_done
            sd zero, (t0)
            addi t0, t0, 8
            j zero_bss

        zero_bss_done:

            // Jump into rust code
            la sp, __stack_end
            j _before_main",
        options(noreturn)
    )
}
