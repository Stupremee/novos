//! Hart local storage and context.

use crate::drivers::{plic, DeviceManager};
use crate::{allocator, page, pmem, unit};
use alloc::boxed::Box;
use alloc::vec;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use devicetree::DeviceTree;

/// The size of each trap stack.
pub const TRAP_STACK_SIZE: usize = 4 * unit::KIB;

/// This structure is replicated on every hart and stores
/// hart-local information like a trap-stack or the hart id.
#[repr(C)]
pub struct HartContext {
    /// The id of this hart, this is our own generated id and is not compatible
    /// with the hart id given by opensbi.
    id: u64,
    /// A pointer to the stack that must be used during interrupts. This pointer will point to
    /// the end of the trap stack inside virtual memory space.
    trap_stack: NonNull<u8>,
    /// Location to temporarily store the stack pointer inside the interrupt handler.
    temp_sp: usize,
    /// Bool indicating if this hart was the booting hart.
    is_bsp: bool,
    fdt: &'static DeviceTree<'static>,
    devices: &'static DeviceManager,
}

impl HartContext {
    /// Return the id of this hart.
    #[inline]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Return if this hart was the booting hart.
    #[inline]
    pub fn is_bsp(&self) -> bool {
        self.is_bsp
    }

    /// Get exclusive access to the global device manager.
    #[inline]
    pub fn devices(&self) -> &'static DeviceManager {
        self.devices
    }

    /// Get access to the global devicetree.
    #[inline]
    pub fn fdt(&self) -> &'static DeviceTree<'static> {
        self.fdt
    }

    /// Get the PLIC context for the current hart.
    pub fn plic_context(&self) -> plic::Context {
        let raw = 1 + 2 * self.id;
        unsafe { plic::Context::new(raw as usize) }
    }
}

/// Get the context for the current hart, but returns None if the hart local context
/// was not initialized yet.
pub fn try_current() -> Option<&'static HartContext> {
    let addr: usize;

    unsafe {
        asm!("csrr {}, sscratch", out(reg) addr);
    }

    (addr != 0).then(|| unsafe { &*(addr as *const _) })
}

/// Get the context for the current hart.
pub fn current() -> &'static HartContext {
    try_current().expect("Hart local context not yet initialized")
}

/// Initializes the context for this hart by allocating memory and then saving
/// the pointer inside the `sscratch` CSR.
pub unsafe fn init_hart_context(
    hart_id: u64,
    is_bsp: bool,
    devices: &'static DeviceManager,
    fdt: &'static DeviceTree<'static>,
) -> Result<(), allocator::Error> {
    // allocate the trap stack
    let mut stack = ManuallyDrop::new(vec![0u8; TRAP_STACK_SIZE]);

    // create the hart context and write it to the page
    let ctx = HartContext {
        id: hart_id,
        trap_stack: NonNull::new(stack.as_mut_ptr().add(TRAP_STACK_SIZE)).unwrap(),
        temp_sp: 0,
        is_bsp,
        devices,
        fdt,
    };

    // box up the context so it's stored on the heap
    let ptr = Box::into_raw(Box::new(ctx));

    // store the address inside the sscratch register to make it
    // available everywhere on this hart
    asm!("csrw sscratch, {}", in(reg) ptr);

    Ok(())
}

/// Initialize the ELF Hart local storage, that can be used via the `thread_local` attribute.
pub unsafe fn init_hart_local_storage() -> Result<(), allocator::Error> {
    // get the range of the tdata section to copy
    let (start, end) = riscv::symbols::tdata_range();
    let len = end as usize - start as usize;

    // allocate new memory for the tdata section
    let order = allocator::order_for_size(len);
    let new = page::phys2virt(pmem::alloc_order(order)?.as_ptr()).as_ptr::<u8>();
    let new = core::slice::from_raw_parts_mut(new, len);

    // copy the memory to the newly allocated data
    let original = core::slice::from_raw_parts(start, len);
    new.copy_from_slice(original);

    // set the thread pointer register
    asm!("mv tp, {}", in(reg) new.as_ptr());

    Ok(())
}
