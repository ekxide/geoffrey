// SPDX-License-Identifier: Apache-2.0

use flexi_logger::{style, DeferredNow, FlexiLoggerError, Logger, LogSpecification};
use yansi::Paint;

fn format(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &log::Record,
) -> Result<(), std::io::Error> {
    let level = record.level();

    let level_text = match level {
        log::Level::Error => "[Error]",
        log::Level::Warn => "[Warn ]",
        log::Level::Info => "[Info ]",
        log::Level::Debug => "[Debug]",
        log::Level::Trace => "[Trace]",
    };

    now.now().format("%Y-%m-%d %H:%M:%S%.3f").fixed(8).dim();

    write!(
        w,
        "{} {} {}",
        now.now().format("%Y-%m-%d %H:%M:%S%.3f").fixed(8).dim(),
        style(level).paint(level_text),
        &record.args()
    )
}

pub fn try_init(log_level: &str) -> Result<(), FlexiLoggerError> {
    Logger::with(LogSpecification::parse(log_level).unwrap())
        .set_palette("9;11;10;7;8".to_owned())
        .format_for_stderr(format)
        .start()?;

    Ok(())
}
