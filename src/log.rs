use std::io::Write;

pub fn init_logging(mut level: clap_verbosity_flag::Verbosity, colored: bool) {
    level.set_default(Some(log::Level::Info));

    if let Some(level) = level.log_level() {
        let mut builder = env_logger::Builder::new();
        builder.write_style(if colored {
            env_logger::WriteStyle::Always
        } else {
            env_logger::WriteStyle::Never
        });

        builder.filter(None, level.to_level_filter());

        if level == log::LevelFilter::Trace {
            builder.format_timestamp_secs();
        } else {
            builder.format(|f, record| {
                writeln!(
                    f,
                    "[{}] {}",
                    record.level().to_string().to_lowercase(),
                    record.args()
                )
            });
        }

        builder.init();
    }
}
