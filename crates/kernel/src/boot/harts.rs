//! Code to bringup all secondary harts.

use crate::{drivers::DeviceManager, hart, page::KernelPageTable};
use devicetree::DeviceTree;
use riscv::csr::satp;

#[repr(C)]
struct HartArgs {
    fdt: &'static DeviceTree<'static>,
    devices: &'static DeviceManager,
}

/// Boot all harts that are present in the given devicetree.
pub(super) unsafe fn boot_all_harts(
    hart_id: usize,
    fdt: &DeviceTree<'_>,
    table: &mut KernelPageTable,
) {
    // prepare the satp register that will be used by the harts
    let satp = satp::Satp {
        asid: 0,
        mode: satp::Mode::Sv39,
        root_table: 0,
    }
    .as_bits();

    // extract all harts that do not have our id from the devicetree
    let cores = fdt
        .find_nodes("/cpus/cpu@")
        .filter_map(|cpu| cpu.prop("reg")?.as_u32())
        .filter(|id| *id as usize != hart_id);

    // go through each hart and try to boot it
    for hart in cores {
        let args = HartArgs {
            devices: hart::current().devices(),
            fdt: hart::current().fdt(),
        };

        // allocate stack in physical memory, since it will be mapped in later by the hart
        let (pstack, vstack) = super::alloc_kernel_stack(table, hart as u64);

        // write the hart arguments to the stack
        let pstack = pstack.as_ptr::<HartArgs>().offset(-1).cast::<usize>();

        let vstack = vstack.as_ptr::<HartArgs>().offset(-1);
        vstack.write(args);
        let vstack = vstack.cast::<usize>();

        // write the satp value on the new stack
        vstack.offset(-1).write(satp);

        // write the virtual stack to the new stack
        vstack.offset(-2).write(vstack as usize);

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
        ld t0, -8(a1)
        ld t1, -16(a1)

        # Enable paging
        csrw satp, t0
        sfence.vma

        # Load the stack and jump into rust code
        mv sp, t1

        # The new stack also points to the hart arguments
        mv a1, t1
        j {}
    ",
        sym rust_hart_entry,
        options(noreturn)
    )
}

unsafe extern "C" fn rust_hart_entry(hart_id: u64, args: &HartArgs) -> ! {
    // initialize hart local storage and hart context
    hart::init_hart_local_storage().unwrap();
    hart::init_hart_context(hart_id, false, args.devices, args.fdt).unwrap();

    // after setting up everything, we're ready to jump into safe rust code
    crate::hmain()
}
