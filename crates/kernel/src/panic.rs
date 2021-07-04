use crate::hart;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

struct PanicPrinter<'panic>(&'panic PanicInfo<'panic>);

impl fmt::Display for PanicPrinter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=========================")?;
        if let Some(ctx) = hart::try_current() {
            writeln!(f, "KERNEL PANIC ON HART {:3}", ctx.id())?;
        } else {
            writeln!(f, "KERNEL PANIC",)?;
        }
        writeln!(f, "=========================")?;

        match (self.0.location(), self.0.message()) {
            (Some(loc), Some(msg)) => {
                write!(f, "line {}, file {}: {}", loc.line(), loc.file(), msg)
            }
            (None, Some(msg)) => {
                write!(f, "{}", msg)
            }
            (Some(loc), None) => {
                write!(f, "line {}, file {}", loc.line(), loc.file())
            }
            (None, None) => write!(f, "no information available."),
        }
    }
}

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    if hart::try_current().map_or(true, |c| c.is_bsp()) {
        // if a panic on the main hart occurrs, don't use the global logger because it might
        // deadlock, instead use directly print to the SBI output.
        struct PanicLogger;
        impl fmt::Write for PanicLogger {
            fn write_str(&mut self, x: &str) -> core::fmt::Result {
                let _ = x.chars().try_for_each(sbi::legacy::put_char);
                Ok(())
            }
        }

        // before printing the panic, shutdown all other harts
        if let Some(hart) = hart::try_current() {
            let mut mask = sbi::HartMask::all_from_base(0);
            mask.mask &= !(1 << hart.id());

            let _ = sbi::ipi::send_ipi(mask);
        }

        let _ = write!(&mut PanicLogger, "{}", PanicPrinter(info));

        sbi::system::fail_shutdown();
    } else {
        log::error!("{}", PanicPrinter(info));

        // try to stop this hart if it paniced
        let _ = sbi::hsm::stop();

        // if we can't stop this hart, just spin it
        loop {
            riscv::asm::wfi()
        }
    }
}

#[alloc_error_handler]
fn alloc_handler(layout: core::alloc::Layout) -> ! {
    panic!(
        "memory allocation of {} bytes and {} alignment failed",
        layout.size(),
        layout.align()
    )
}
