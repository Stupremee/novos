pub mod ns16550a;
pub mod plic;

use crate::hart;
use crate::page::{self, PageSize, PageTable, Perm};
use core::fmt;
use devicetree::{node::Node, DeviceTree};
use plic::ClaimGuard;
use riscv::sync::{Mutex, MutexGuard};

/// Macro for registering a list of device drivers.
/// It generates multiple methods to get any driver instance
/// from a node.
macro_rules! declare_drivers {
    ($($name:ident: $dev:path),+) => {
        /// Enum containing every type of device that can be parsed from a node.
        enum Device {
            $($name($dev)),+
        }

        /// Try to get *any* device from the given node.
        fn device_from_node(node: &Node<'_>) -> Option<Device> {
            $(if <$dev as DeviceDriver>::compatible_with(&node) {
                <$dev as DeviceDriver>::from_node(node).map(Device::$name)
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

    fn as_interruptable(&mut self) -> Option<&mut dyn Interruptable> {
        None
    }
}

pub trait Interruptable {
    /// This method is called inside an external interrupt, if the interrupt has
    /// the id that was previously returned by `interrupt_id` method.
    ///
    /// If this method returns `Ok(())`, the interrupt will be marked as completed.
    fn handle_interrupt(&mut self, id: u32) -> Result<(), &'static str>;

    /// Return the id of the interrupt this interruptable can handle.
    fn interrupt_id(&self) -> u32;
}

declare_drivers![Plic: plic::Controller, Uart: ns16550a::Device];

/// Manages multiples device drivers.
pub struct DeviceManager {
    plic: Mutex<Option<plic::Controller>>,
    uart: Mutex<Option<ns16550a::Device>>,
}

impl DeviceManager {
    /// Creates a new device manager that will iterate all nodes in the devicetree and
    /// add all drivers found for the nodes.
    pub fn from_devicetree(tree: &DeviceTree<'_>) -> Self {
        let mut plic = Mutex::new(None);
        let mut uart = Mutex::new(None);

        'next_dev: for node in tree.nodes() {
            match device_from_node(&node) {
                Some(Device::Plic(dev)) => plic = Mutex::new(Some(dev)),
                Some(Device::Uart(dev)) => uart = Mutex::new(Some(dev)),
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

        Self { plic, uart }
    }

    /// Initalize all devices inside this device manager.
    pub unsafe fn init(&self) {
        // firstly, initialize the PLIC since it will be used
        // to register interrupts if possible.
        let mut plic = self.plic.lock();
        if let Some(plic) = plic.as_mut() {
            plic.init();
        }

        // note that this will initialize the uart device twice, but i dont care
        // so just do it
        if let Some(uart) = self.uart.lock().as_mut() {
            uart.init();

            if let Some((uart, plic)) = uart.as_interruptable().zip(plic.as_mut()) {
                let id = uart.interrupt_id();
                plic.enable(hart::current().plic_context(), id);
                plic.set_threshold(hart::current().plic_context(), 0);
                plic.set_priority(id, 1);
            }
        }
    }

    /// Handle an external interrupt.
    pub fn handle_interrupt(&self, irq: ClaimGuard<'_>) -> Result<(), &'static str> {
        // check if the uart device supports interrupts
        if let Some(uart) = self.uart.lock().as_mut().and_then(|d| d.as_interruptable()) {
            uart.handle_interrupt(irq.id())?;

            // on success, finish the interrupt and return
            irq.finish();
            return Ok(());
        }

        Ok(())
    }

    /// Get exclusive access to the PLIC.
    pub fn plic(&self) -> MutexGuard<'_, Option<plic::Controller>> {
        self.plic.lock()
    }

    /// Get exclusive access to the UART driver.
    pub fn uart(&self) -> MutexGuard<'_, Option<ns16550a::Device>> {
        self.uart.lock()
    }
}

/// A global logger that uses the hart local context to access the UART port.
pub struct GlobalLog;

impl fmt::Write for GlobalLog {
    fn write_str(&mut self, x: &str) -> fmt::Result {
        let devices = hart::current().devices();
        if let Some(uart) = &mut *devices.uart() {
            uart.write_str(x)?;
        }

        Ok(())
    }
}

impl log::Logger for GlobalLog {
    fn hart_id(&self) -> Option<usize> {
        Some(hart::current().id() as usize)
    }
}
