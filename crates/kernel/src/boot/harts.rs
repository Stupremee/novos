//! Code to bringup all secondary harts.

use crate::{hart, page, trap};
use devicetree::DeviceTree;

#[repr(C)]
struct HartArgs {
    satp: u64,
    vstack: u64,
    boot_hart: u64,
    fdt: DeviceTree<'static>,
}

/// Boot all harts that are present in the given devicetree.
pub(super) unsafe fn boot_all_harts(hart_id: usize, fdt: DeviceTree<'_>, satp: u64) {
    // extract all harts that do not have our id from the devicetree
    let cores = fdt
        .find_nodes("/cpus/cpu@")
        .filter_map(|cpu| cpu.prop("reg")?.as_u32())
        .filter(|id| *id as usize != hart_id);

    let mut table = page::root();

    // go through each hart and try to boot it
    for hart in cores {
        // allocate stack in physical memory, since it will be mapped in later by the hart
        let (pstack, vstack) = super::alloc_kernel_stack(&mut *table, hart as u64);

        // write the required arguments onto the new stack
        let pstack = pstack.as_ptr::<HartArgs>().offset(-1);
        let vstack = vstack.as_ptr::<HartArgs>().offset(-1);

        let args = HartArgs {
            fdt: hart::current().fdt(),
            boot_hart: hart_id as u64,
            satp,
            vstack: vstack as u64,
        };

        vstack.write(args);

        // try to start the hart
        match sbi::hsm::start(hart as usize, hart_entry as usize, pstack as usize) {
            Ok(()) => {}
            Err(err) => log::warn!("{} to boot hart {}: {:?}", "Failed".yellow(), hart, err),
        }
    }
}

#[naked]
unsafe extern "C" fn hart_entry(_hart_id: usize, _sp: usize) -> ! {
    asm!(
        "
        # Load the global pointer into
        # the `gp` register
        .option push
        .option norelax
            lla gp, __global_pointer$
        .option pop

        # Load arguments from stack
        #   t0: satp
        #   t1: virtual stack
        ld t0, 0(a1)
        ld t1, 8(a1)

        # Load the virtual stack and arguments
        mv sp, t1
        mv a1, t1

        # Jump into rust code by enabling paging and trapping to the function
        lla t1, {}
        csrw stvec, t1

        sfence.vma
        nop
    ",
        sym rust_hart_entry,
        options(noreturn)
    )
}

#[repr(align(4))]
unsafe extern "C" fn rust_hart_entry(hart_id: u64, args: &HartArgs) -> ! {
    log::debug!("HI");
    loop {}
    // initialize hart local storage and hart context
    //hart::init_hart_local_storage().unwrap();
    //hart::init_hart_context(hart_id, args.boot_hart, args.fdt).unwrap();

    //// install trap handler
    //trap::install_handler();

    //// after setting up everything, we're ready to jump into safe rust code
    crate::hmain()
}
