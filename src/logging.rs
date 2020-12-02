pub fn init_logger(pattern: &str) {
    use ansi_term::Colour;
    use chrono::Utc;
    use std::io::Write;

    let mut builder = env_logger::Builder::new();
    if let Ok(lvl) = std::env::var("RUST_LOG") {
        builder.parse_filters(&lvl);
    }
    builder.parse_filters(pattern);

    builder.format(move |buf, record| {
        let time_now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let output = format!("{} {}", Colour::Blue.bold().paint(time_now), record.args(),);
        writeln!(buf, "{}", output)
    });

    if builder.try_init().is_err() {
        log::info!("Global logger already initialized.  Skipping");
    }
}
