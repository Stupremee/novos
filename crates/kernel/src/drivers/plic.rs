//! Driver for the official RISC-V Program Level Interrupt Controller.

use core::ptr::NonNull;
use devicetree::node::Node;

pub struct Device {
    _base: NonNull<u8>,
}

impl super::DeviceDriver for Device {
    fn compatible_with(node: &Node<'_>) -> bool {
        node.compatible_with("riscv,plic0")
    }

    fn from_node(_: &Node<'_>) -> Option<Self> {
        Some(Self {
            _base: NonNull::dangling(),
        })
    }

    fn init(&mut self) {
        todo!()
    }
}
