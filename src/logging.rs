// SPDX-License-Identifier: Apache-2.0

use flexi_logger::{style, DeferredNow, FlexiLoggerError, Logger};
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

    write!(
        w,
        "{} {} {}",
        Paint::fixed(8, now.now().format("%Y-%m-%d %H:%M:%S%.3f")).dimmed(),
        style(level, level_text),
        &record.args()
    )
}

pub fn try_init(log_level: &str) -> Result<(), FlexiLoggerError> {
    Logger::with_str(log_level)
        .set_palette("9;11;10;7;8".to_owned())
        .format_for_stderr(format)
        .start()?;

    Ok(())
}
