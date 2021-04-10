pub mod ns16550a;
pub mod plic;

use crate::hart;
use crate::page::{self, PageSize, PageTable, Perm};
use alloc::boxed::Box;
use devicetree::{node::Node, DeviceTree};
use plic::ClaimGuard;

/// Macro for registering a list of device drivers.
/// It generates multiple methods to get any driver instance
/// from a node.
macro_rules! declare_drivers {
    ($($name:ident: $dev:path),+) => {
        /// Enum containing every type of device that can be parsed from a node.
        pub enum Device {
            $($name($dev)),+
        }

        impl Device {
            /// Try to get *any* device from the given node.
            pub fn from_node(node: &Node<'_>) -> Option<Device> {
                $(if <$dev as DeviceDriver>::compatible_with(&node) {
                    <$dev as DeviceDriver>::from_node(node).map(Device::$name)
                } else)+ {
                    None
                }
            }

            /// Turn this device type into a reference to a `DeviceDriver` trait object.
            pub fn as_device_driver(&self) -> &dyn DeviceDriver {
                match self {
                    $(Device::$name(x) => x,)+
                }
            }

            /// Turn this device type into a boxed `DeviceDriver` trait object.
            pub fn into_device_driver(self) -> Box<dyn DeviceDriver> {
                match self {
                    $(Device::$name(x) => Box::new(x),)+
                }
            }
        }
    };
}

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

declare_drivers![Plic: plic::Controller, Uart: ns16550a::Device];

/// Manages multiples device drivers.
pub struct DeviceManager {
    plic: Option<plic::Controller>,
    log: Option<Device>,
}

impl DeviceManager {
    /// Creates a new device manager that will iterate all nodes in the devicetree and
    /// add all drivers found for the nodes.
    pub fn from_devicetree(tree: &DeviceTree<'_>) -> Self {
        let mut plic = None;
        let mut log = None;

        'next_dev: for node in tree.nodes() {
            match Device::from_node(&node) {
                Some(Device::Plic(dev)) => plic = Some(dev),
                Some(dev @ Device::Uart(_)) => log = Some(dev),
                // skip any node that doesn't has a driver
                None => continue,
            };

            // if this is a valid device, we need to map it's mmio space
            for reg in node.regions() {
                // loop through each page and map it
                for page in (reg.start()..reg.end()).step_by(PageSize::Kilopage.size()) {
                    let res = page::root().map(
                        page.into(),
                        page.into(),
                        PageSize::Kilopage,
                        Perm::READ | Perm::WRITE,
                    );

                    match res {
                        Ok(()) => {}
                        Err(err) => {
                            log::warn!(
                                "{} to map MMIO space for {}: {}. Skipping device...",
                                "Failed".yellow(),
                                node.name(),
                                err
                            );
                            continue 'next_dev;
                        }
                    }
                }
            }
        }

        Self { plic, log }
    }

    /// Return an iterator that will iterate over all devices that are registered inside
    /// this device manager.
    pub fn devices(&self) -> impl Iterator<Item = &dyn DeviceDriver> {
        let mut plic: Option<&dyn DeviceDriver> =
            self.plic.as_ref().map(|x| x as &dyn DeviceDriver);
        let mut log: Option<&dyn DeviceDriver> = self.log.as_ref().map(Device::as_device_driver);

        core::iter::from_fn(move || plic.take().or_else(|| log.take()))
    }

    /// Initalize all devices inside this device manager.
    pub unsafe fn init(&self) {
        // init all devices
        self.devices().for_each(|dev| dev.init());

        // if theres a PLIC, enable interrupts for all devices that support interrupts
        if let Some(plic) = self.plic() {
            let ctx = hart::current().plic_context();
            plic.set_threshold(ctx, 0);

            self.devices()
                .filter_map(|d| d.as_interruptable())
                .for_each(|dev| {
                    let id = dev.interrupt_id();
                    plic.enable(ctx, id);
                    plic.set_priority(id, 1);
                });
        }
    }

    /// Handle an external interrupt.
    pub fn handle_interrupt(&self, irq: ClaimGuard<'_>) -> Result<(), &'static str> {
        // find the device that handles the given interrupt
        let dev = self
            .devices()
            .filter_map(|dev| dev.as_interruptable())
            .find(|dev| dev.interrupt_id() == irq.id());

        if let Some(dev) = dev {
            dev.handle_interrupt(irq.id())?;

            // on success, finish the interrupt
            irq.finish();
        }

        Ok(())
    }

    /// Get access to the PLIC.
    pub fn plic(&self) -> Option<&plic::Controller> {
        self.plic.as_ref()
    }

    /// Get access to the logger, if any was found.
    pub fn logger(&self) -> Option<&dyn log::Logger> {
        self.log
            .as_ref()
            .and_then(|dev| dev.as_device_driver().as_logger())
    }
}
