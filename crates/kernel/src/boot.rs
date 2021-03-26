//! Kernel entrypoint and everything related to boot into the kernel

use crate::allocator::{order_for_size, size_for_order, PAGE_SIZE};
use crate::drivers::{ns16550a, DeviceDriver};
use crate::{
    drivers, hart, interrupt,
    page::{self, PageSize, PageTable, Perm, PhysAddr, VirtAddr},
    pmem, unit, StaticCell,
};
use alloc::boxed::Box;
use core::slice;
use devicetree::DeviceTree;
use riscv::{csr::satp, symbols};

static PAGE_TABLE: StaticCell<page::sv39::Table> = StaticCell::new(page::sv39::Table::new());

/// The base virtual addresses where the stacks for every hart are located.
pub const KERNEL_STACK_BASE: usize = 0x001D_DD00_0000;

/// The stack size for each hart.
pub const KERNEL_STACK_SIZE: usize = 1024 * 1024;

/// The virtual address at which the physical memory is mapped in, such that adding
/// this constant to any "real" physaddr returns the new physaddr which can be used if
/// paging is activaed.
pub const KERNEL_PHYS_MEM_BASE: usize = 0x001F_FF00_0000;

/// The base virtual address where the allocator will start allocating virtual memory.
pub const KERNEL_VMEM_ALLOC_BASE: usize = 0x0000_AA00_0000;

/// The maximum number of harts that will try to be started.
pub const HART_COUNT: usize = 4;

/// Allocates the stack for a hart, with the given id and returns the end address
/// of the new stack.
///
/// Returns both, the physical and virtual address to the end of the stack.
fn alloc_kernel_stack(table: &mut page::sv39::Table, id: u64) -> (PhysAddr, VirtAddr) {
    // calculate the start address for hart `id`s stack
    let start = KERNEL_STACK_BASE + (id as usize * (KERNEL_STACK_SIZE + PAGE_SIZE));
    let guard_start = start + KERNEL_STACK_SIZE;

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

    // map the guard page
    table
        .map(0.into(), guard_start.into(), PageSize::Kilopage, Perm::EXEC)
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
    let stdout = fdt.chosen().stdout();
    if let Some(mut uart) = stdout
        .filter(|n| ns16550a::Device::compatible_with(n))
        .and_then(|node| ns16550a::Device::from_node(&node))
    {
        uart.init();

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
        };
    }

    // initialize the physmem allocator
    pmem::init(&fdt).unwrap();

    // get access to the global page table
    let table = &mut *PAGE_TABLE.get();

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

    let gp: usize;
    asm!("mv {}, gp", out(reg) gp);

    // jump to rust code using the trampoline
    entry_trampoline(
        hart_id,
        page::phys2virt(&fdt as *const _).as_ptr(),
        stack.into(),
        rust_trampoline as usize,
        page::phys2virt(gp).into(),
    )
}

/// Trampoline to jump enable paging and transition to new stack.
#[naked]
unsafe extern "C" fn entry_trampoline(
    _hart_id: usize,
    _fdt: *const DeviceTree<'_>,
    _new_stack: usize,
    _dst: usize,
    _gp: usize,
) -> ! {
    #[rustfmt::skip]
    asm!("
        mv gp, a4

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
unsafe extern "C" fn rust_trampoline(hart_id: usize, fdt: &DeviceTree<'_>) -> ! {
    // install the interrupt handler
    interrupt::install_handler();

    // initialize all device drivers
    let devices = Box::leak(Box::new(drivers::DeviceManager::from_devicetree(fdt)));
    devices.init();

    // initialize hart local storage and hart context
    hart::init_hart_context(hart_id as u64, true, devices).unwrap();
    log::info!("{} with id {} online", "Hart".green(), hart::current().id());

    // switch to the new logger
    if log::init_log(drivers::GlobalLog).is_err() {
        panic!("Failed to switch to new global logger");
    }

    let mut table = page::root();

    // bring up all other harts
    for cpu in fdt
        .cpus()
        .children()
        .filter(|node| node.name().starts_with("cpu@"))
    {
        // get the id for the hart
        let id = cpu.prop("reg").unwrap().as_u32().unwrap();

        // skip ourself
        if hart_id == id as usize {
            continue;
        }

        // allocate the stack for the new hart
        let (_phys_stack, virt_stack) = alloc_kernel_stack(&mut *table, id.into());

        // we need to pass the device manager to the new hart
        virt_stack
            .as_ptr::<usize>()
            .offset(-1)
            .write_volatile(devices as *const _ as usize);

        match sbi::hsm::start(id as usize, hart_entry as usize, virt_stack.into()) {
            Ok(()) => {}
            Err(err) => {
                log::warn!("{} to start hart {}: {:?}", "Failed".yellow(), id, err);
            }
        }
    }
    // explicitly drop, otherwise the table lock would never be dropped
    drop(table);

    crate::main(fdt);

    loop {
        riscv::asm::wfi();
    }
}

/// The entrypoint for the other harts.
#[naked]
unsafe extern "C" fn hart_entry(_hart_id: usize, _virt_stack: usize) -> ! {
    asm!(
        "
            // Load global pointer
            la t0, __global_pointer$
            li t1, {}
            sub t0, t0, t1

        .option push
        .option norelax
            mv gp, t0
        .option pop


            mv sp, a1

            // Enable paging
            la t0, {}
            srli t0, t0, 12

            li t1, 8
            slli t1, t1, 60
            or t0, t1, t0

            csrw satp, t0
            sfence.vma

            ld a1, -8(sp)

            // Jump to rust code
            j {}
         ", const KERNEL_PHYS_MEM_BASE, sym PAGE_TABLE, sym rust_hart_entry,
        options(noreturn)
    )
}

/// The rust entry point for each additional hart.
unsafe extern "C" fn rust_hart_entry(
    hart_id: usize,
    devices: &'static drivers::DeviceManager,
) -> ! {
    // install the interrupt handler
    interrupt::install_handler();

    // init hart local context
    hart::init_hart_context(hart_id as u64, false, devices).unwrap();

    log::info!("{} with ID {} online", "Hart".green(), hart::current().id(),);

    crate::hmain();

    loop {
        riscv::asm::wfi();
    }
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
