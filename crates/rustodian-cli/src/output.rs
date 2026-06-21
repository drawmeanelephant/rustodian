//! Output formatting and tracing initialization.

use tracing_subscriber::EnvFilter;

/// Initialize tracing with the given verbosity level.
///
/// - 0: warn
/// - 1: info
/// - 2: debug
/// - 3+: trace
pub fn init_tracing(verbosity: u8) {
    let filter = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
}
