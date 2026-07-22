//! Logging infrastructure (Part 1, Section 10).
//!
//! Uses `tracing` rather than `log`/`env_logger` — the eventual Main Loop
//! (Part 19) and background consolidation work (Section 4a) will want
//! structured, span-aware tracing for async background processes, and
//! setting this up now avoids a migration later.
//!
//! Two independent sinks are configured:
//!   1. General application log — stdout + a daily-rotating file under
//!      `<data_dir>/logs/alynxis.log.<date>`.
//!   2. Dedicated admin-override audit log — `<data_dir>/logs/admin_override.log`,
//!      append-only (never rotated), capturing every admin authentication
//!      attempt and every privileged action taken under an admin session
//!      (Section 3c: "every use of the admin override must be logged").
//!      Kept separate from the general log so Lynx can review admin
//!      activity in isolation. Only events explicitly tagged with target
//!      `"alynxis::admin_override"` (see `core::admin`) are routed here —
//!      every function in `core::admin` that matters logs at `info` level
//!      or above under that target, so it always passes through even a
//!      relatively quiet global filter level.

use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Must be kept alive for the lifetime of the process — dropping it stops
/// the non-blocking file writers from flushing pending log lines.
/// `main.rs` holds this in a local binding for the process's whole run.
pub struct LoggingGuards {
    _app_guard: WorkerGuard,
    _admin_guard: WorkerGuard,
}

pub fn init_logging(logs_dir: &Path, log_level: &str) -> LoggingGuards {
    std::fs::create_dir_all(logs_dir).expect("failed to create logs directory");

    let app_file_appender = tracing_appender::rolling::daily(logs_dir, "alynxis.log");
    let (app_file_writer, app_guard) = tracing_appender::non_blocking(app_file_appender);

    let admin_file_appender = tracing_appender::rolling::never(logs_dir, "admin_override.log");
    let (admin_file_writer, admin_guard) = tracing_appender::non_blocking(admin_file_appender);

    let env_filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_ansi(true)
        .with_writer(std::io::stdout);

    let app_file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(app_file_writer);

    let admin_file_layer = fmt::layer()
        .with_target(true)
        .with_ansi(false)
        .with_writer(admin_file_writer)
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.target().starts_with("alynxis::admin_override")
        }));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(app_file_layer)
        .with(admin_file_layer)
        .init();

    LoggingGuards {
        _app_guard: app_guard,
        _admin_guard: admin_guard,
    }
}
