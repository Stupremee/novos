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
    (guard = $guard:expr; $($args:tt)+) => {
        $crate::log!(guard = $guard; Info, $($args)+);
    };

    ($($args:tt)+) => {
        $crate::log!(Info, $($args)+);
    };
}

/// Log a warn message.
#[macro_export]
macro_rules! warn {
    (guard = $guard:expr; $($args:tt)+) => {
        $crate::log!(guard = $guard; Warn, $($args)+);
    };

    ($($args:tt)+) => {
        $crate::log!(Warn, $($args)+);
    };
}

/// Log an error message.
#[macro_export]
macro_rules! error {
    (guard = $guard:expr; $($args:tt)+) => {
        $crate::log!(guard = $guard; Error, $($args)+);
    };

    ($($args:tt)+) => {
        $crate::log!(Error, $($args)+);
    };
}

/// The stadnard logging macro.
#[macro_export]
macro_rules! log {
    (guard = $guard:expr; $level:ident, $($args:tt)+) => {{
        #[allow(unused_imports)]
        use $crate::__export::owo_colors::OwoColorize;
        $crate::log::<$crate::$level>($guard, ::core::module_path!(), ::core::format_args!($($args)*));
    }};

    ($level:ident, $($args:tt)+) => {{
        let mut _guard = $crate::global_log();
        $crate::log!(guard = &mut _guard; $level, $($args)+);
    }};
}
