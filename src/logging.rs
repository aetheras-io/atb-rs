use std::path::PathBuf;

use tracing_appender::{
    non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::Rotation,
};
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self, Layer as FmtLayer, format},
    layer::SubscriberExt,
};

pub use tracing_appender;
pub use tracing_subscriber;

pub struct TraceOpts {
    pub filters: Option<String>,
    pub buffer: usize,
    pub lossy: bool,
    pub json: bool,
    /// When present, logs are written to a rolling file instead of stdout.
    pub file: Option<FileSinkOpts>,
}

#[derive(Clone)]
pub struct FileSinkOpts {
    pub directory: PathBuf,
    pub file_name: String,
    pub rotation: Rotation,
}

impl Default for FileSinkOpts {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("./target/logs"),
            file_name: "app.log".to_string(),
            rotation: Rotation::DAILY,
        }
    }
}

impl Default for TraceOpts {
    fn default() -> Self {
        Self {
            filters: None,
            buffer: 20_000,
            lossy: false,
            json: false,
            file: None,
        }
    }
}

/// Easiest path: dev/prod aware, non-blocking stdout, JSON in prod, pretty in dev.
/// Uses 20k buffer, non-lossy; returns WorkerGuard if we successfully became global.
pub fn init_tracer(opts: TraceOpts) -> anyhow::Result<WorkerGuard> {
    let (writer, guard) = build_nonblocking(&opts);
    let env_filter = opts
        .filters
        .as_ref()
        .map(|f| EnvFilter::builder().parse_lossy(f))
        .unwrap_or_else(EnvFilter::from_default_env);

    if opts.json {
        let subscriber = Registry::default()
            .with(loud_layer_json().with_writer(writer))
            .with(env_filter);
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let subscriber = Registry::default()
            .with(
                loud_pretty_layer()
                    .with_ansi(opts.file.is_none()) // stdout keeps color; files disable below
                    .with_writer(writer),
            )
            .with(env_filter);
        tracing::subscriber::set_global_default(subscriber)?;
    }
    Ok(guard)
}

/// Backwards compatibility shim: file logger with defaults.
pub fn init_file_tracer() -> anyhow::Result<WorkerGuard> {
    init_tracer(TraceOpts {
        file: Some(FileSinkOpts::default()),
        json: true,
        ..Default::default()
    })
}

// Loud pretty (text) formatter.
// Concrete type: FmtLayer<Registry, DefaultFields, Format<Full, SystemTime>, fn() -> Stdout>
pub fn loud_pretty_layer() -> FmtLayer<Registry> {
    fmt::layer()
        .with_span_events(format::FmtSpan::CLOSE)
        .with_target(true)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(true)
        .with_thread_ids(true)
        .with_thread_names(true)
}

/// JSON / prod formatter
pub fn loud_layer_json() -> FmtLayer<Registry, format::JsonFields, format::Format<format::Json>> {
    fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_events(fmt::format::FmtSpan::CLOSE)
        .with_span_list(true)
        .with_target(true)
        .with_line_number(true)
        .with_file(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_ansi(false)
        .flatten_event(true)
}

/// Build writer + guard + EnvFilter in one place so stdout/file paths stay aligned.
/// Build a non-blocking writer (stdout or rolling file) based on TraceOpts.
pub fn build_nonblocking(opts: &TraceOpts) -> (NonBlocking, WorkerGuard) {
    let builder = NonBlockingBuilder::default()
        .buffered_lines_limit(opts.buffer)
        .lossy(opts.lossy);
    if let Some(file) = &opts.file {
        let dir = file.directory.clone();
        let _ = std::fs::create_dir_all(&dir);

        let file_name = file.file_name.clone();
        let file_appender = match file.rotation {
            Rotation::NEVER => tracing_appender::rolling::never(&dir, file_name),
            Rotation::HOURLY => tracing_appender::rolling::hourly(&dir, file_name),
            Rotation::DAILY => tracing_appender::rolling::daily(&dir, file_name),
            Rotation::MINUTELY => tracing_appender::rolling::minutely(&dir, file_name),
        };

        builder.finish(file_appender)
    } else {
        builder.finish(std::io::stdout())
    }
}

#[deprecated(note = "use init_tracer, this will be removed soon")]
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
            writeln!(buf, "{payload}")
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
        // Avoid noisy panics when another logger/subscriber is already set.
        // Use `trace` to minimize noise in production.
        log::trace!("Global logger already initialized. Skipping env_logger init");
    }
}
