//! Hart local storage and context.

use crate::pmem;
use core::cell::Cell;

/// This structure is replicated on every hart and stores
/// hart-local information like a trap-stack or the hart id.
#[repr(C)]
pub struct HartContext {
    /// The id of this hart, this is our own generated id and is not compatible
    /// with the hart id given by opensbi.
    id: Cell<u64>,
}

impl HartContext {
    /// Return the id of this hart.
    ///
    /// This is our own generated id and thus not compatible
    /// with the hart id given by opensbi.
    #[inline]
    pub fn id(&self) -> u64 {
        self.id.get()
    }
}

/// Get the context for the current hart.
pub fn current() -> &'static HartContext {
    let addr: usize;

    unsafe {
        asm!("csrr {}, sscratch", out(reg) addr);
        &*(addr as *const _)
    }
}

/// Initializes the context for this hart by allocating memory and then saving
/// the pointer inside the `sscratch` CSR.
pub unsafe fn init_hart_context(hart_id: u64) -> Result<(), allocator::Error> {
    // allocate the memory for the context
    let page = pmem::alloc()?;

    // create the hart context and write it to the page
    let ctx = HartContext {
        id: Cell::new(hart_id),
    };
    core::ptr::write_volatile(page.as_ptr().cast(), ctx);

    // store the address inside the sscratch register to make it
    // available everywhere on this hart
    asm!("csrw sscratch, {}", in(reg) page.as_ptr());

    Ok(())
}
