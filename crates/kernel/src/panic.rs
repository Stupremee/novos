use crate::hart;
use core::fmt;
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
    log::error!("{}", PanicPrinter(info));

    if hart::try_current().map_or(true, |c| c.is_bsp()) {
        sbi::system::fail_shutdown();
    } else {
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
