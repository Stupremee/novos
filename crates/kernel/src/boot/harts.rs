//! Code to bringup all secondary harts.

use super::KERNEL_STACK_SIZE;
use crate::{allocator, hart, pmem};
use devicetree::DeviceTree;

/// Boot all harts that are present in the given devicetree.
pub(super) unsafe fn boot_all_harts(hart_id: usize, fdt: &DeviceTree<'_>) {
    // extract all harts that do not have our id from the devicetree
    let cores = fdt
        .find_nodes("/cpus/cpu@")
        .filter_map(|cpu| cpu.prop("reg")?.as_u32())
        .filter(|id| *id as usize != hart_id);

    // go through each hart and try to boot it
    for hart in cores {
        // allocate stack in physical memory, since it will be mapped in later by the hart
        let stack_order = allocator::order_for_size(KERNEL_STACK_SIZE);
        let stack = pmem::alloc_order(stack_order).unwrap();

        // try to start the hart
        match sbi::hsm::start(hart as usize, hart_entry as usize, stack.as_ptr() as usize) {
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
        
        # Load the stack and jump into rust code
        mv sp, a1
        j {}
    ",
        sym rust_hart_entry,
        options(noreturn)
    )
}

unsafe extern "C" fn rust_hart_entry(hart_id: usize) -> ! {
    log::debug!("HELLO FROM {}", hart_id);

    // initialize hart local storage and hart context
    hart::init_hart_local_storage().unwrap();

    // after setting up everything, we're ready to jump into safe rust code
    crate::hmain()
}
