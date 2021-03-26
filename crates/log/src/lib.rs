//! Logging system for the kernel.

#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![feature(unsize, ptr_metadata)]
#![no_std]

mod value;
pub use value::Value;

#[macro_use]
mod macros;

use core::fmt::{self, Write};
use core::marker::PhantomData;
use core::time::Duration;
use owo_colors::{colors, Color, OwoColorize};

use riscv::sync::Mutex;

const LOGGER_SIZE: usize = 8;
static LOG: GlobalLogger = GlobalLogger(Mutex::new(None));

struct GlobalLogger(Mutex<Option<Value<dyn Logger, { LOGGER_SIZE }>>>);

unsafe impl Send for GlobalLogger {}
unsafe impl Sync for GlobalLogger {}

#[doc(hidden)]
pub mod __export {
    pub use owo_colors;
}

/// Represents anything that can be used to log the log events to some output.
pub trait Logger: Write {
    /// Return the id of the current hart.
    ///
    /// If this returns None, the hart id wont be printed
    fn hart_id(&self) -> Option<usize>;
}

/// Represents any level of a log message.
pub trait Level {
    type Color: Color;

    const NAME: &'static str;
}

/// The debug log level.
pub enum Debug {}
impl Level for Debug {
    type Color = colors::Magenta;
    const NAME: &'static str = "Debug";
}

/// The info log level.
pub enum Info {}
impl Level for Info {
    type Color = colors::Cyan;
    const NAME: &'static str = "Info";
}

/// The warn log level.
pub enum Warn {}
impl Level for Warn {
    type Color = colors::Yellow;
    const NAME: &'static str = "Warn";
}

/// The error log level.
pub enum Error {}
impl Level for Error {
    type Color = colors::Red;
    const NAME: &'static str = "Error";
}

struct LogWriter<'fmt, L> {
    prefix: bool,
    time: Duration,
    module: &'fmt str,
    _guard: &'fmt mut dyn Logger,
    _level: PhantomData<L>,
}

impl<L: Level> LogWriter<'_, L> {
    fn print_prefix(&mut self) -> fmt::Result {
        let secs = self.time.as_secs();
        let millis = self.time.subsec_millis();

        struct DisplayId(Option<usize>);
        impl fmt::Display for DisplayId {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if let Some(id) = self.0 {
                    write!(f, "[{}] ", id)?;
                }

                Ok(())
            }
        }

        write!(
            self._guard,
            "{} {}{:>5} {}> ",
            format_args!("[{:>3}.{:<03}]", secs, millis).dimmed(),
            DisplayId(self._guard.hart_id()).dimmed(),
            L::NAME.fg::<L::Color>(),
            self.module,
        )
    }
}

impl<L: Level> fmt::Write for LogWriter<'_, L> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.prefix {
            self.print_prefix()?;
            self.prefix = false;
        }

        if let Some(newline) = s.find('\n') {
            let (s, rest) = s.split_at(newline + 1);
            self._guard.write_str(s)?;

            if !rest.is_empty() {
                self.print_prefix()?;
                self._guard.write_str(rest)?;
            } else {
                self.prefix = true;
            }
        } else {
            self._guard.write_str(s)?;
        }

        Ok(())
    }
}

#[doc(hidden)]
pub fn log<L: Level>(module: &str, args: fmt::Arguments<'_>) {
    if let Some(log) = &mut *LOG.0.lock() {
        let mut writer = LogWriter {
            time: riscv::asm::time(),
            prefix: true,
            module,
            _guard: &mut **log,
            _level: PhantomData::<L>,
        };

        writeln!(writer, "{}", args).expect("failed to log message");
    }
}

/// Initializes the global logger.
///
/// Returns `Ok` on success, and `Err` with the given logger if the logger was already initialized,
/// or the given logger was to big to be put into a global.
pub fn init_log<L: Logger + Send + Sync + 'static>(log: L) -> Result<(), L> {
    let mut lock = LOG.0.lock();
    let val = Value::<dyn Logger, { LOGGER_SIZE }>::new(log)?;
    *lock = Some(val);

    Ok(())
}
