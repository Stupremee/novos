pub mod ns16550a;
pub mod plic;

use crate::page::{self, PageSize, PageTable, Perm};
use alloc::boxed::Box;
use alloc::vec::Vec;
use devicetree::{node::Node, DeviceTree};

/// Macro for registering a list of device drivers.
/// It generates multiple methods to get any driver instance
/// from a node.
macro_rules! declare_drivers {
    ($($dev:path),+) => {
        /// Try to get *any* device from the given node.
        fn device_from_node(node: &Node<'_>) -> Option<Box<dyn DeviceDriver>> {
            $(if <$dev as DeviceDriver>::compatible_with(&node) {
                <$dev as DeviceDriver>::from_node(node).map(|dev| Box::new(dev) as Box<dyn DeviceDriver>)
            } else)+ {
                None
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
        Self: Sized,
    {
        None
    }

    /// Initializes this device driver.
    fn init(&mut self);
}

declare_drivers![plic::Device];

/// Manages multiples device drivers.
pub struct DeviceManager {
    devices: Vec<Box<dyn DeviceDriver>>,
}

impl DeviceManager {
    /// Creates a new device manager that will iterate all nodes in the devicetree and
    /// add all drivers found for the nodes.
    pub fn from_devicetree(tree: &DeviceTree<'_>) -> Self {
        let mut devices = Vec::new();

        'next_dev: for node in tree.nodes() {
            let dev = match device_from_node(&node) {
                Some(dev) => dev,
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

            // if we reach this, the regions were successfully mapped
            devices.push(dev);
        }

        Self { devices }
    }

    /// Initalize all devices inside this device manager.
    pub fn init(&mut self) {
        self.devices.iter_mut().for_each(|dev| dev.init())
    }
}
