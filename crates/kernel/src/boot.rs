//! Kernel entrypoint and everything related to boot into the kernel

use crate::drivers::{ns16550a, DeviceTreeDriver};
use crate::{
    hart,
    page::{self, PageSize, PageTable, Perm},
    pmem, unit, StaticCell,
};
use allocator::{order_for_size, size_for_order};
use core::mem::MaybeUninit;
use core::slice;
use devicetree::DeviceTree;
use riscv::symbols;

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
struct HartArgs {
    id: u64,
    stack: *const u8,
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
        for page in (start as usize..end as usize).step_by(allocator::PAGE_SIZE) {
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

    // map the stack that we will use for this hart at the global stack base
    table
        .map_alloc(
            KERNEL_STACK_BASE.into(),
            KERNEL_STACK_SIZE / allocator::PAGE_SIZE,
            PageSize::Kilopage,
            Perm::READ | Perm::WRITE,
        )
        .unwrap();

    // construct the raw value for the satp register we give to the trampoline
    let satp = table as *const _ as usize;
    let satp = (8 << 60) | (satp >> 12);

    log::info!(
        "{:#x?}",
        table.translate((KERNEL_STACK_BASE + KERNEL_STACK_SIZE - 8).into())
    );

    // prepare some addresses that are used inside the trampoline
    entry_trampoline(
        hart_id,
        &fdt,
        satp,
        KERNEL_STACK_BASE + KERNEL_STACK_SIZE - 8,
        rust_trampoline as usize,
    )
}

/// Trampoline to jump enable paging and transition to new stack.
#[naked]
unsafe extern "C" fn entry_trampoline(
    _hart_id: usize,
    _fdt: *const DeviceTree<'_>,
    _satp: usize,
    _new_stack: usize,
    _dst: usize,
) -> ! {
    #[rustfmt::skip]
    asm!("
        // Enable paging
        csrw satp, a2

        mv t0, sp
        mv sp, a3
        mv t1, a3
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
        jr a4
    ",
    options(noreturn));
}

/// Wrapper around the `main` call to avoid marking `main` as `extern "C"`
unsafe extern "C" fn rust_trampoline(_hart_id: usize, fdt: &DeviceTree<'_>) -> ! {
    // bring up the other harts before jumping to the main
    // this is done here so we already have paging enabled
    let page = pmem::alloc().unwrap();
    let args = &mut *page::phys2virt(page.as_ptr())
        .as_ptr::<[MaybeUninit<HartArgs>; HART_COUNT as usize]>();

    // check if there's enough space to fit all hart arguments
    assert!(HART_COUNT as usize * core::mem::size_of::<HartArgs>() <= allocator::PAGE_SIZE);

    // try to boot every hart
    let mut id = 1;

    for (sbi_id, args_ptr) in (1..HART_COUNT).zip(args.iter_mut()) {
        let args = HartArgs {
            id,
            stack: page::phys2virt(pmem::alloc().unwrap().as_ptr()).as_ptr(),
        };
        args_ptr.as_mut_ptr().write(args);

        match sbi::hsm::start(
            sbi_id as usize,
            hart_entry as usize,
            args_ptr.as_ptr() as usize,
        ) {
            Ok(_) => id += 1,
            // the hart is non-existant
            Err(sbi::Error::InvalidParam) | Err(sbi::Error::AlreadyAvailable) => {}
            Err(err) => {} // log::warn!("{} to start hart {}: {:?}", "Failed".yellow(), sbi_id, err),
        };
    }

    crate::main(fdt);
    sbi::system::shutdown()
}

#[naked]
unsafe extern "C" fn hart_entry(args: &'static HartArgs) -> ! {
    asm!("
    2: j 2b
        ld sp, 8(a1)
        j {}
    ", sym rust_hart_entry, options(noreturn))
}

#[no_mangle]
unsafe extern "C" fn rust_hart_entry(args: &'static HartArgs) -> ! {
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
