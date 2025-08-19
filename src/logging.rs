pub fn init_tracer(filters: Option<&str>, is_dev: bool) {
    use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

    // Prefer explicit filters; fall back to RUST_LOG or defaults.
    let env_filter = if let Some(f) = filters {
        EnvFilter::builder().parse_lossy(f)
    } else {
        EnvFilter::from_default_env()
    };

    if is_dev {
        // Human-friendly console logs for local development.
        let fmt_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_line_number(true)
            .with_file(true)
            .with_ansi(true)
            .with_span_events(fmt::format::FmtSpan::CLOSE);

        let subscriber = Registry::default().with(env_filter).with(fmt_layer);
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to set tracing default subscriber");
    } else {
        // Production: structured JSON logs (one JSON object per line).
        let fmt_layer = fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(true)
            .with_target(true)
            .with_line_number(true)
            .with_file(true)
            .with_thread_ids(true)
            .with_thread_names(true)
            .with_ansi(false)
            .flatten_event(true)
            .with_span_events(fmt::format::FmtSpan::CLOSE);

        let subscriber = Registry::default().with(env_filter).with(fmt_layer);
        tracing::subscriber::set_global_default(subscriber)
            .expect("failed to set tracing default subscriber");
    }
}

use tracing_appender::non_blocking;
pub fn init_file_tracer() -> non_blocking::WorkerGuard {
    use tracing_appender::rolling;
    use tracing_subscriber::{fmt, layer::SubscriberExt, Registry};

    // Daily rotation: target/logs/app.log.YYYY-MM-DD
    let file_appender = rolling::daily("./target/logs", "app.log");
    let (non_blocking_writer, guard) = non_blocking(file_appender);

    // Allow override for file log level via RUST_LOG_FILE; default to info.
    let env_filter = std::env::var("RUST_LOG_FILE")
        .ok()
        .map(|s| tracing_subscriber::EnvFilter::builder().parse_lossy(&s))
        .unwrap_or_else(|| tracing_subscriber::EnvFilter::new("info"));

    // Structured JSON logs to file with span context.
    let fmt_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_target(true)
        .with_line_number(true)
        .with_file(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_ansi(false)
        .flatten_event(true)
        .with_writer(non_blocking_writer);

    let subscriber = Registry::default().with(env_filter).with(fmt_layer);

    // Note: Only the first call to set_global_default succeeds.
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set tracing default subscriber");
    guard
}

pub fn init_logger(pattern: &str, deep: bool) {
    // Keep env_logger for users of the `log` facade; add optional JSON output for prod use.
    use ansi_term::Colour;
    use chrono::Utc;
    use log::Level;
    use std::io::Write;

    let mut builder = env_logger::Builder::new();

    // Pattern takes lowest precedence; RUST_LOG overrides.
    builder.parse_filters(pattern);
    if let Ok(lvl) = std::env::var("RUST_LOG") {
        builder.parse_filters(&lvl);
    }

    // Toggle JSON output via RUST_LOG_JSON=1/true.
    let json_enabled = std::env::var("RUST_LOG_JSON")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if json_enabled {
        builder.format(move |buf, record| {
            use serde_json::json;
            let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let payload = json!({
                "ts": ts,
                "level": record.level().to_string(),
                "target": record.target(),
                "module_path": record.module_path(),
                "file": record.file(),
                "line": record.line(),
                "msg": record.args().to_string(),
            });
            writeln!(buf, "{}", payload.to_string())
        });
    } else if deep {
        builder.format(move |buf, record| {
            let time_now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let level = match record.level() {
                Level::Error => Colour::Red.bold().paint("ERR"),
                Level::Warn => Colour::Yellow.bold().paint("WRN"),
                Level::Info => Colour::Green.bold().paint("INF"),
                Level::Debug => Colour::Cyan.bold().paint("DBG"),
                Level::Trace => Colour::White.bold().paint("TRC"),
            };
            let output = format!(
                "[{}] {} {}\n  {}|{}:{}",
                level,
                Colour::Blue.bold().paint(time_now),
                record.args(),
                record.module_path().unwrap_or("UNKNOWN_MODULE"),
                record.file().unwrap_or("UNKNOWN_FILE"),
                record.line().unwrap_or(0),
            );
            writeln!(buf, "{output}")
        });
    } else {
        builder.format(move |buf, record| {
            let time_now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let level = match record.level() {
                Level::Error => Colour::Red.bold().paint("ERR"),
                Level::Warn => Colour::Yellow.bold().paint("WRN"),
                Level::Info => Colour::Green.bold().paint("INF"),
                Level::Debug => Colour::Cyan.bold().paint("DBG"),
                Level::Trace => Colour::White.bold().paint("TRC"),
            };
            let output = format!(
                "[{}] {} {}",
                level,
                Colour::Blue.bold().paint(time_now),
                record.args(),
            );
            writeln!(buf, "{output}")
        });
    };

    if builder.try_init().is_err() {
        log::info!("Global logger already initialized. Skipping");
    }
}
