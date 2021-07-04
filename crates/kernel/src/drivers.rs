pub mod ns16550a;
pub mod plic;

use devicetree::node::Node;

/// A device / driver that can be configured and found inside the devicetree.
pub trait DeviceDriver {
    /// Check if this device driver is compatible with the given node.
    ///
    /// If this returns true, `from_node` will be called with this node.
    fn compatible_with(_: &Node<'_>) -> bool
    where
        Self: Sized;

    /// Instantiate this device using a node from the devicetree, which was found using the
    /// compatible with method.
    ///
    /// This method should not initialize this device, and instead only create an instance of it.
    fn from_node(_: &Node<'_>) -> Option<Self>
    where
        Self: Sized;

    /// Initializes this device driver.
    ///
    /// # Safety
    ///
    /// This method must only be called once.
    unsafe fn init(&self);

    /// Convert this device driver, into a driver that supports interrupts.
    ///
    /// Not every device may support interrupts, so this returns an `Option` indicating the presence
    /// of interrupts.
    fn as_interruptable(&self) -> Option<&dyn Interruptable> {
        None
    }

    /// Turn self into a logger instance, if this device is able to be a logger.
    fn as_logger(&self) -> Option<&dyn log::Logger> {
        None
    }
}

/// Represents any device that supports external interrupts.
pub trait Interruptable: DeviceDriver {
    /// This method is called inside an external interrupt, if the interrupt has
    /// the id that was previously returned by `interrupt_id` method.
    ///
    /// If this method returns `Ok(())`, the interrupt will be marked as completed.
    fn handle_interrupt(&self, id: u32) -> Result<(), &'static str>;

    /// Return the id of the interrupt this interruptable can handle.
    fn interrupt_id(&self) -> u32;
}
