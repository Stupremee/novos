//! Structure for representing traps and exceptions.

/// The bit that is set to `1`, inside the `cause` value, if a trap is
/// an interrupt.
pub const INTERRUPT_BIT: usize = 1 << (usize::BITS - 1);

/// All different kinds of traps.
#[derive(Debug, Copy, Clone)]
pub enum Trap {
    UserSoftwareInterrupt,
    SupervisorSoftwareInterrupt,
    MachineSoftwareInterrupt,
    UserTimerInterrupt,
    SupervisorTimerInterrupt,
    MachineTimerInterrupt,
    UserExternalInterrupt,
    SupervisorExternalInterrupt,
    MachineExternalInterrupt,

    InstructionAddressMisaligned,
    InstructionAccessFault,
    IllegalInstruction,
    Breakpoint,
    LoadAddressMisaligned,
    LoadAccessFault,
    StoreAddressMisaligned,
    StoreAccessFault,
    UserModeEnvironmentCall,
    SupervisorModeEnvironmentCall,
    MachineModeEnvironmentCall,
    InstructionPageFault,
    LoadPageFault,
    StorePageFault,

    /// Special value that indicates an invalid cause,
    /// that may be valid in the future.
    Reserved,
}

impl Trap {
    /// Converts a raw cause number coming from the `scause` register,
    /// into a [`Trap`].
    pub fn from_cause(cause: usize) -> Option<Self> {
        use Trap::*;

        const NON_INTERRUPT_TABLE: [Trap; 16] = [
            InstructionAddressMisaligned,
            InstructionAccessFault,
            IllegalInstruction,
            Breakpoint,
            LoadAddressMisaligned,
            LoadAccessFault,
            StoreAddressMisaligned,
            StoreAccessFault,
            UserModeEnvironmentCall,
            SupervisorModeEnvironmentCall,
            Reserved,
            MachineModeEnvironmentCall,
            InstructionPageFault,
            LoadPageFault,
            Reserved,
            StorePageFault,
        ];

        const INTERRUPT_TABLE: [Trap; 12] = [
            UserSoftwareInterrupt,
            SupervisorSoftwareInterrupt,
            Reserved,
            MachineSoftwareInterrupt,
            UserTimerInterrupt,
            SupervisorTimerInterrupt,
            Reserved,
            MachineTimerInterrupt,
            UserExternalInterrupt,
            SupervisorExternalInterrupt,
            Reserved,
            MachineExternalInterrupt,
        ];

        if cause & INTERRUPT_BIT != 0 {
            let cause = cause & !INTERRUPT_BIT;
            INTERRUPT_TABLE.get(cause).copied()
        } else {
            NON_INTERRUPT_TABLE.get(cause).copied()
        }
    }
}
