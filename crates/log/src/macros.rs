/// Log a debug message.
#[macro_export]
macro_rules! debug {
    (guard = $guard:expr; $($args:tt)+) => {
        $crate::log!(guard = $guard; Debug, $($args)+);
    };

    ($($args:tt)+) => {
        $crate::log!(Debug, $($args)+);
    };
}

/// Log an info message.
#[macro_export]
macro_rules! info {
    ($($args:tt)+) => {
        $crate::log!(Info, $($args)+);
    };
}

/// Log a warn message.
#[macro_export]
macro_rules! warn {
    ($($args:tt)+) => {
        $crate::log!(Warn, $($args)+);
    };
}

/// Log an error message.
#[macro_export]
macro_rules! error {
    ($($args:tt)+) => {
        $crate::log!(Error, $($args)+);
    };
}

/// The stadnard logging macro.
#[macro_export]
macro_rules! log {
    ($level:ident, $($args:tt)+) => {{
        #[allow(unused_imports)]
        use $crate::__export::owo_colors::OwoColorize;
        $crate::log::<$crate::$level>(::core::module_path!(), ::core::format_args!($($args)*));
    }};
}

/// Custom implementation of the `dbg` macro.
#[macro_export]
macro_rules! dbg {
    () => {
        $crate::debug!("[{}:{}]", ::core::file!(), ::core::line!());
    };

    ($val:expr $(,)?) => {
        match $val {
            tmp => {
                $crate::debug!("[{}:{}] {} = {:#x?}", ::core::file!(), ::core::line!(),
                    ::core::stringify!($val), &tmp);
                tmp
            }
        }
    };

    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}
