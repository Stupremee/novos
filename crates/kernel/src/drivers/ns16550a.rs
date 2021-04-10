//! Driver for the NS16550a UART Chip.

use core::{fmt, ptr::NonNull};
use devicetree::node::Node;

pub struct Device {
    interrupt_id: u32,
    base: NonNull<u8>,
}

impl Device {
    /// Initialize this UART driver.
    pub fn init(&self) {
        let ptr = self.base.as_ptr();
        unsafe {
            // First, enable FIFO by setting the first bit of the FCR
            // register to `1`.
            let fcr = ptr.offset(2);
            fcr.write_volatile(0x01);

            // Set the buffer size to 8-bits, by writing
            // setting the two low bits in the LCR register to `1`.
            let lcr_value = 0x03;
            let lcr = ptr.offset(3);
            lcr.write_volatile(lcr_value);

            // Enable received data available interrupt,
            // by writing `1` into the IER register.
            let ier = ptr.offset(1);
            ier.write_volatile(0x01);

            // "Calculating" the divisor required for the baud rate.
            let divisor = 592u16;
            let divisor = divisor.to_le();

            // To write the actual divisor, we need to enable
            // the divisor latch enable bit, that is located
            // in the LCR register at bit `7`.
            lcr.write_volatile(1 << 7 | lcr_value);

            // Now write the actual divisor value into the first two bytes
            ptr.cast::<u16>().write_volatile(divisor);

            // After writing divisor, switch back to normal mode
            // and disable divisor latch.
            lcr.write_volatile(lcr_value);
        }
    }

    /// Tries to read incoming data.
    ///
    /// Returns `None` if there's currently no data available.
    pub fn try_read(&self) -> Option<u8> {
        self.data_ready().then(|| unsafe { self.read_data() })
    }

    /// Spins the hart until new data is available.
    pub fn read(&self) -> u8 {
        while !self.data_ready() {}

        // SAFETY
        // We only reach this code after data is ready
        unsafe { self.read_data() }
    }

    /// Tries to write data into the transmitter.
    ///
    /// Returns `Some(x)`, containing the given `x`, if the transmitter is not ready.
    pub fn try_write(&self, x: u8) -> Option<u8> {
        if self.transmitter_empty() {
            // SAFETY
            // We checked if the transmitter is empty
            unsafe {
                self.write_data(x);
            }
            None
        } else {
            Some(x)
        }
    }

    /// Spins this hart until the given data can be written.
    pub fn write(&self, x: u8) {
        while !self.transmitter_empty() {}

        // SAFETY
        // We only reach this code if the transmitter is empty.
        unsafe {
            self.write_data(x);
        }
    }

    /// Reads data from the data register.
    ///
    /// # Safety
    ///
    /// Must only be called if data is available.
    unsafe fn read_data(&self) -> u8 {
        let ptr = self.base.as_ptr();
        ptr.read_volatile()
    }

    /// Writes data to the data register.
    ///
    /// # Safety
    ///
    /// Must only be called if the transmitter is ready.
    unsafe fn write_data(&self, x: u8) {
        let ptr = self.base.as_ptr();
        ptr.write_volatile(x)
    }

    fn transmitter_empty(&self) -> bool {
        unsafe {
            // The transmitter ready bit inside the LSR register indicates
            // if the transmitter is empty and ready to send new data.
            let lsr = self.base.as_ptr().offset(5);
            let value = lsr.read_volatile();

            value & (1 << 6) != 0
        }
    }

    fn data_ready(&self) -> bool {
        unsafe {
            // The data ready bit inside the LSR register indicates
            // if there's data available.
            let lsr = self.base.as_ptr().offset(5);
            let value = lsr.read_volatile();

            value & 0x01 != 0
        }
    }
}

unsafe impl Send for Device {}
unsafe impl Sync for Device {}

impl log::Logger for Device {
    fn write_str(&self, s: &str) -> fmt::Result {
        for x in s.bytes() {
            self.write(x);
        }
        Ok(())
    }
}

impl super::DeviceDriver for Device {
    fn compatible_with(node: &Node<'_>) -> bool {
        node.compatible_with("ns16550a")
    }

    fn from_node(node: &Node<'_>) -> Option<Self> {
        let base = node.regions().next()?.start();
        let uart = Device {
            base: NonNull::new(base as *mut _)?,
            interrupt_id: node.prop("interrupts")?.as_u32()?,
        };
        Some(uart)
    }

    unsafe fn init(&self) {
        Device::init(self)
    }

    fn as_interruptable(&self) -> Option<&dyn super::Interruptable> {
        Some(self)
    }

    fn as_logger(&self) -> Option<&dyn log::Logger> {
        Some(self)
    }
}

impl super::Interruptable for Device {
    fn handle_interrupt(&self, _: u32) -> Result<(), &'static str> {
        // drain the input buffer
        while let Some(c) = self.try_read() {
            if c == b'S' {
                sbi::system::shutdown();
            }
        }

        Ok(())
    }

    fn interrupt_id(&self) -> u32 {
        self.interrupt_id
    }
}
