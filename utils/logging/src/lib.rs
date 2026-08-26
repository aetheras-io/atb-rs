use std::path::PathBuf;

use tracing_appender::{
    non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{RollingFileAppender, Rotation},
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

/// Initialize tracing. Keep the returned guard alive until shutdown.
#[must_use = "keep the returned WorkerGuard alive or logs may be dropped"]
pub fn init_tracer(opts: TraceOpts) -> anyhow::Result<WorkerGuard> {
    let (writer, guard) = build_nonblocking(&opts);
    let env_filter = opts
        .filters
        .as_ref()
        .map(|filters| EnvFilter::builder().parse_lossy(filters))
        .unwrap_or_else(|| {
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
        });

    if opts.json {
        let subscriber = Registry::default()
            .with(noisy_layer_json().with_writer(writer))
            .with(env_filter);
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let subscriber = Registry::default()
            .with(
                noisy_pretty_layer()
                    .with_ansi(opts.file.is_none())
                    .with_writer(writer),
            )
            .with(env_filter);
        tracing::subscriber::set_global_default(subscriber)?;
    }
    Ok(guard)
}

/// Initialize a JSON rolling-file tracer with default settings.
#[must_use = "keep the returned WorkerGuard alive or logs may be dropped"]
pub fn init_file_tracer() -> anyhow::Result<WorkerGuard> {
    init_tracer(TraceOpts {
        file: Some(FileSinkOpts::default()),
        json: true,
        ..Default::default()
    })
}

/// Build a non-blocking stdout or rolling-file writer.
pub fn build_nonblocking(opts: &TraceOpts) -> (NonBlocking, WorkerGuard) {
    let builder = NonBlockingBuilder::default()
        .buffered_lines_limit(opts.buffer)
        .lossy(opts.lossy);
    if let Some(file) = &opts.file {
        builder.finish(RollingFileAppender::new(
            file.rotation.clone(),
            &file.directory,
            &file.file_name,
        ))
    } else {
        builder.finish(std::io::stdout())
    }
}

pub fn noisy_pretty_layer() -> FmtLayer<Registry> {
    fmt::layer()
        .with_span_events(format::FmtSpan::CLOSE)
        .with_target(true)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(true)
        .with_thread_ids(true)
        .with_thread_names(true)
}

pub fn noisy_layer_json() -> FmtLayer<Registry, format::JsonFields, format::Format<format::Json>> {
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

#[deprecated(note = "use init_tracer; this will be removed in a future major release")]
pub fn init_logger(pattern: &str, deep: bool) {
    use ansi_term::Colour;
    use chrono::Utc;
    use std::io::Write;

    let mut builder = env_logger::Builder::new();
    builder.parse_filters(pattern);
    if let Ok(level) = std::env::var("RUST_LOG") {
        builder.parse_filters(&level);
    }

    let json_enabled = std::env::var("RUST_LOG_JSON")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if json_enabled {
        builder.format(move |buffer, record| {
            use serde_json::json;
            let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            let payload = json!({
                "ts": timestamp,
                "level": record.level().to_string(),
                "target": record.target(),
                "module_path": record.module_path(),
                "file": record.file(),
                "line": record.line(),
                "msg": record.args().to_string(),
            });
            writeln!(buffer, "{payload}")
        });
    } else if deep {
        builder.format(move |buffer, record| {
            let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let level = colored_level(record.level());
            writeln!(
                buffer,
                "[{level}] {} {}\n  {}|{}:{}",
                Colour::Blue.bold().paint(timestamp),
                record.args(),
                record.module_path().unwrap_or("UNKNOWN_MODULE"),
                record.file().unwrap_or("UNKNOWN_FILE"),
                record.line().unwrap_or(0),
            )
        });
    } else {
        builder.format(move |buffer, record| {
            let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let level = colored_level(record.level());
            writeln!(
                buffer,
                "[{level}] {} {}",
                Colour::Blue.bold().paint(timestamp),
                record.args(),
            )
        });
    }

    if builder.try_init().is_err() {
        log::trace!("global logger already initialized; skipping initialization");
    }
}

fn colored_level(level: log::Level) -> ansi_term::ANSIString<'static> {
    use ansi_term::Colour;

    match level {
        log::Level::Error => Colour::Red.bold().paint("ERR"),
        log::Level::Warn => Colour::Yellow.bold().paint("WRN"),
        log::Level::Info => Colour::Green.bold().paint("INF"),
        log::Level::Debug => Colour::Cyan.bold().paint("DBG"),
        log::Level::Trace => Colour::White.bold().paint("TRC"),
    }
}
