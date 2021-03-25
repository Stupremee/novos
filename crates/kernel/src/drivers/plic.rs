//! Driver for the official RISC-V Program Level Interrupt Controller.

use core::marker::PhantomData;
use devicetree::node::Node;
use voladdress::{Safe, VolAddress, VolBlock, VolSeries};

/// The number of contexts available.
const CONTEXT_COUNT: usize = 15872;

/// The default priority each interrupt gets at initialization.
const DEFAULT_PRIORITY: u32 = 1;

pub struct Controller {
    /// The maximum number of interrupts available.
    max_interrupts: usize,
    /// The priorities for each interrupt source.
    priorities: VolBlock<u32, Safe, Safe, 1024>,
    /// Enable / Disable global interrupts for each context.
    enable: VolBlock<u32, Safe, Safe, { CONTEXT_COUNT * 32 }>,
    /// The threshold values and claim bits
    threshold_claim: VolSeries<u32, Safe, Safe, CONTEXT_COUNT, 4096>,
}

impl Controller {
    /// Enable the interrupt with `id` for the given context.
    pub fn enable(&mut self, ctx: usize, id: usize) {
        assert_ne!(id, 0, "interrupt with id 0 is invalid");

        // find the entry and bit to modify
        let (entry, bit) = Self::enable_idx_bit(ctx, id);

        // set the bit in `entry` to 1
        let addr = self.enable.index(entry);
        let val = addr.read() | (1 << bit);
        addr.write(val);
    }

    /// Disable the interrupt with `id` for the given context.
    pub fn disable(&mut self, ctx: usize, id: usize) {
        assert_ne!(id, 0, "interrupt with id 0 is invalid");

        // find the entry and bit to modify
        let (entry, bit) = Self::enable_idx_bit(ctx, id);

        // set the bit in `entry` to 0
        let addr = self.enable.index(entry);
        let val = addr.read() & !(1 << bit);
        addr.write(val);
    }

    /// Claim an interrupt, if it's pending, and return a guard that can be used
    /// to finish the interrupt.
    pub fn claim(&mut self, ctx: usize) -> Option<ClaimGuard<'_>> {
        let claim = unsafe { self.threshold_claim.index(ctx).add(1) };

        match claim.read() {
            0 => None,
            id => Some(ClaimGuard {
                id,
                claim,
                _lifetime: PhantomData,
            }),
        }
    }

    /// Set the threshold for the given context.
    pub fn set_threshold(&mut self, ctx: usize, threshold: u32) {
        let addr = self.threshold_claim.index(ctx);
        addr.write(threshold);
    }

    /// Set the priority of the interrupt with `id`.
    pub fn set_priority(&mut self, id: usize, priority: u32) {
        assert_ne!(id, 0, "interrupt with id 0 is invalid");
        self.priorities.index(id).write(priority);
    }

    /// Get the entry index and bit for a context, interrupt-id pair
    fn enable_idx_bit(ctx: usize, id: usize) -> (usize, usize) {
        let entry = (ctx * 32) + (id / 32);
        let bit = id % 32;
        (entry, bit)
    }
}

impl super::DeviceDriver for Controller {
    fn compatible_with(node: &Node<'_>) -> bool {
        node.compatible_with("riscv,plic0")
    }

    fn from_node(node: &Node<'_>) -> Option<Self> {
        let base = node.regions().next()?.start();
        let max_interrupts = node.prop("riscv,ndev")?.as_u32()? as usize;

        unsafe {
            Some(Self {
                max_interrupts,
                priorities: VolBlock::new(base),
                enable: VolBlock::new(base + 0x2000),
                threshold_claim: VolSeries::new(base + 0x200000),
            })
        }
    }

    fn init(&mut self) {
        // set the default priority for each interrupt
        for id in 1..self.max_interrupts {
            self.priorities.index(id).write(DEFAULT_PRIORITY)
        }

        log::info!("{} the PLIC", "Initialized".green());
    }

    fn as_plic(&mut self) -> Option<&mut Controller> {
        Some(self)
    }
}

/// Guard that can be used to finish an interrupt.
pub struct ClaimGuard<'plic> {
    /// The interrupt id
    id: u32,
    /// Address to the claim register for finsishing this interrupt.
    claim: VolAddress<u32, Safe, Safe>,
    _lifetime: PhantomData<&'plic ()>,
}

impl ClaimGuard<'_> {
    /// Return the id of this interrupt claim.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Finish this interrupt.
    pub fn finish(self) {
        self.claim.write(self.id);
    }
}
