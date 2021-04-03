pub fn init_logger(pattern: &str) {
    use ansi_term::Colour;
    use chrono::Utc;
    use log::Level;
    use std::io::Write;

    let mut builder = env_logger::Builder::new();
    builder.parse_filters(pattern);
    if let Ok(lvl) = std::env::var("RUST_LOG") {
        builder.parse_filters(&lvl);
    }

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
        writeln!(buf, "{}", output)
    });

    if builder.try_init().is_err() {
        log::info!("Global logger already initialized.  Skipping");
    }
}
