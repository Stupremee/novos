//! Trap handler

use riscv::{csr, trap::Trap};

/// Installs the global trap handler by writing it's address
/// into the stvec register.
///
/// This method will also enable external interrupts and timer interrupts.
pub fn install_handler() {
    let addr = _trap_handler as usize;
    unsafe {
        // Tell the CPU that our trap handler is at `addr`
        csr::stvec::write(addr);

        // Enable external interrupts
        csr::sie::set((1 << 9) | (1 << 5) | (1 << 1));
        csr::sstatus::set(1 << 1);
    }
}

/// The rust trap handler
///
/// The returned value will be the new `sepc` value.
pub extern "C" fn trap_handler(
    _frame: &mut TrapFrame,
    scause: usize,
    stval: usize,
    sepc: usize,
) -> usize {
    let cause = match Trap::from_cause(scause) {
        Some(x) => x,
        None => panic!(
            "Invalid trap cause: {:x?} pc: {:#x?} tval: {:#x?}",
            scause, sepc, stval
        ),
    };

    match cause {
        Trap::SupervisorExternalInterrupt => {
            // If there is a PLIC, and it has a pending interrupt,
            // we pass it on to our devicemanager which will redirect the interrupt
            // to the corresponding device.
            //let dev = hart::current().devices();
            //if let Some(irq) = dev
            //.plic()
            //.as_mut()
            //.and_then(|p| p.claim(hart::current().plic_context()))
            //{
            //match hart::current().devices().handle_interrupt(irq) {
            //Ok(()) => {}
            //Err(err) => {
            //log::warn!("{} to run interrupt handler: {}", "Failed".yellow(), err)
            //}
            //}
            //}
        }
        Trap::SupervisorTimerInterrupt => {
            log::debug!("got timer interrupt");
        }
        trap => panic!(
            "Unhandled trap: {:?} pc: {:#x?} tval: {:#x?}",
            trap, sepc, stval
        ),
    };

    sepc
}

/// The global trap handler that will save the registers and then
/// jump to the rist code.
#[naked]
#[repr(align(4))]
unsafe extern "C" fn _trap_handler() -> ! {
    asm!(
        "
        // We don't want nested interrupts, so disable them
        csrci sstatus, 2

        csrrw s0, sscratch, s0

        // Load the interrupt stack
        sd sp, 16(s0)
        ld sp, 8(s0)
        addi sp, sp, -512

        // Store the registers inside the trap frame
        sd x1, 0(sp)

        // Store the old sp
        ld x1, 16(s0)
        sd x1, 8(sp)

        csrrw s0, sscratch, s0

        sd x3, 16(sp)
        sd x4, 24(sp)
        sd x5, 32(sp)
        sd x6, 40(sp)
        sd x7, 48(sp)
        sd x8, 56(sp)
        sd x9, 64(sp)
        sd x10, 72(sp)
        sd x11, 80(sp)
        sd x12, 88(sp)
        sd x13, 96(sp)
        sd x14, 104(sp)
        sd x15, 112(sp)
        sd x16, 120(sp)
        sd x17, 128(sp)
        sd x18, 136(sp)
        sd x19, 144(sp)
        sd x20, 152(sp)
        sd x21, 160(sp)
        sd x22, 168(sp)
        sd x23, 176(sp)
        sd x24, 184(sp)
        sd x25, 192(sp)
        sd x26, 200(sp)
        sd x27, 208(sp)
        sd x28, 216(sp)
        sd x29, 224(sp)
        sd x30, 232(sp)
        sd x31, 240(sp)

        // Floating point registers
        fsd f0, 248(sp)
        fsd f1, 256(sp)
        fsd f2, 264(sp)
        fsd f3, 272(sp)
        fsd f4, 280(sp)
        fsd f5, 288(sp)
        fsd f6, 296(sp)
        fsd f7, 304(sp)
        fsd f8, 312(sp)
        fsd f9, 320(sp)
        fsd f10, 328(sp)
        fsd f11, 336(sp)
        fsd f12, 344(sp)
        fsd f13, 352(sp)
        fsd f14, 360(sp)
        fsd f15, 368(sp)
        fsd f16, 376(sp)
        fsd f17, 384(sp)
        fsd f18, 392(sp)
        fsd f19, 400(sp)
        fsd f20, 408(sp)
        fsd f21, 416(sp)
        fsd f22, 424(sp)
        fsd f23, 432(sp)
        fsd f24, 440(sp)
        fsd f25, 448(sp)
        fsd f26, 456(sp)
        fsd f27, 464(sp)
        fsd f28, 472(sp)
        fsd f29, 480(sp)
        fsd f30, 488(sp)
        fsd f31, 496(sp)

        frcsr t0
        sd t0, 504(sp)

        // Re-enable interrupts after we jumped backed to user code
        li t0, 1 << 5
        csrs sstatus, t0

        // Prepare arguments for jump into Rust code
        mv a0, sp
        csrr a1, scause
        csrr a2, stval
        csrr a3, sepc

        // Call rust handler
        call {}
        csrw sepc, a0

        // Restore registers from trap frame
        ld t0, 504(sp)
        fscsr t0

        ld x1, 0(sp)
        ld x3, 16(sp)
        ld x4, 24(sp)
        ld x5, 32(sp)
        ld x6, 40(sp)
        ld x7, 48(sp)
        ld x8, 56(sp)
        ld x9, 64(sp)
        ld x10, 72(sp)
        ld x11, 80(sp)
        ld x12, 88(sp)
        ld x13, 96(sp)
        ld x14, 104(sp)
        ld x15, 112(sp)
        ld x16, 120(sp)
        ld x17, 128(sp)
        ld x18, 136(sp)
        ld x19, 144(sp)
        ld x20, 152(sp)
        ld x21, 160(sp)
        ld x22, 168(sp)
        ld x23, 176(sp)
        ld x24, 184(sp)
        ld x25, 192(sp)
        ld x26, 200(sp)
        ld x27, 208(sp)
        ld x28, 216(sp)
        ld x29, 224(sp)
        ld x30, 232(sp)
        ld x31, 240(sp)

        // Floating point registers
        fld f0, 248(sp)
        fld f1, 256(sp)
        fld f2, 264(sp)
        fld f3, 272(sp)
        fld f4, 280(sp)
        fld f5, 288(sp)
        fld f6, 296(sp)
        fld f7, 304(sp)
        fld f8, 312(sp)
        fld f9, 320(sp)
        fld f10, 328(sp)
        fld f11, 336(sp)
        fld f12, 344(sp)
        fld f13, 352(sp)
        fld f14, 360(sp)
        fld f15, 368(sp)
        fld f16, 376(sp)
        fld f17, 384(sp)
        fld f18, 392(sp)
        fld f19, 400(sp)
        fld f20, 408(sp)
        fld f21, 416(sp)
        fld f22, 424(sp)
        fld f23, 432(sp)
        fld f24, 440(sp)
        fld f25, 448(sp)
        fld f26, 456(sp)
        fld f27, 464(sp)
        fld f28, 472(sp)
        fld f29, 480(sp)
        fld f30, 488(sp)
        fld f31, 496(sp)

        // Restore stack pointer
        ld sp, 8(sp)

        // Jump out of the interrupt handler
        sret
    ", sym trap_handler,
        options(noreturn)
    )
}

/// The trap frame contains all registers that were stored prior to executing
/// the interrupt handler, and will be loaded again after the interrupt handler.
#[repr(C)]
pub struct TrapFrame {
    pub xregs: XRegisters,
    pub fregs: FRegisters,
}

#[repr(C)]
pub struct XRegisters {
    pub ra: usize,
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub s0: usize,
    pub s1: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,
}

#[repr(C)]
pub struct FRegisters {
    pub f0: usize,
    pub f1: usize,
    pub f2: usize,
    pub f3: usize,
    pub f4: usize,
    pub f5: usize,
    pub f6: usize,
    pub f7: usize,
    pub f8: usize,
    pub f9: usize,
    pub f10: usize,
    pub f11: usize,
    pub f12: usize,
    pub f13: usize,
    pub f14: usize,
    pub f15: usize,
    pub f16: usize,
    pub f17: usize,
    pub f18: usize,
    pub f19: usize,
    pub f20: usize,
    pub f21: usize,
    pub f22: usize,
    pub f23: usize,
    pub f24: usize,
    pub f25: usize,
    pub f26: usize,
    pub f27: usize,
    pub f28: usize,
    pub f29: usize,
    pub f30: usize,
    pub f31: usize,
    pub fscr: usize,
}
