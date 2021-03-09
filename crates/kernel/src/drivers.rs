pub mod ns16550a;

use devicetree::node::{ChosenNode, Node};

/// A device / driver that can be configured and found inside the devicetree.
pub trait DeviceTreeDriver: Sized {
    /// List of names that this device is compatible with.
    ///
    /// This will be used to find the node inside the devicetree.
    const COMPATIBLE: &'static [&'static str];

    /// Instantiate this device using a node from the devicetree, which was found using the
    /// compatible with list.
    fn from_node(_: Node<'_>) -> Option<Self> {
        None
    }

    /// Instantiate this device using the `/chosen` node from the devicetree.
    fn from_chosen(_: ChosenNode<'_>) -> Option<Self> {
        None
    }
}
