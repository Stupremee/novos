//! Code to bringup all secondary harts.

use crate::hart;

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
    // initialize hart local storage and hart context
    hart::init_hart_local_storage().unwrap();

    // we need a new page table, since there will be one page table per core

    // after setting up everything, we're ready to jump into safe rust code
    crate::hmain()
}
