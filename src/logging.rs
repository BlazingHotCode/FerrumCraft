//! Structured logging for engine and gameplay systems.
//!
//! The logger is initialized once at startup and accepts standard `log` crate
//! macros (`info!`, `warn!`, `error!`, `debug!`, `trace!`). Each log entry
//! includes a category string (the Rust module path by default), the severity
//! level, the message, and an HH:MM:SS.mmm timestamp.
//!
//! Logs are written to stdout and to the system app data directory
//! (`<APP_DATA>/FerrumCraft/logs/ferrumcraft_<timestamp>.log`).
//! The log directory is created automatically on first run.
//!
//! Environment variable `FERRUM_LOG` controls the minimum level:
//!   `error`, `warn`, `info`, `debug`, `trace`
//!
//! Example:
//! ```ignore
//! info!(target: "renderer", "Surface configured");
//! warn!("Swapchain lost, reconfiguring");
//! ```

use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::fs;
use std::io::Write;
use std::sync::Mutex;

/// The single global logger instance.
static LOGGER: FerrumLogger = FerrumLogger::new();

/// File handle for writing log output to disk.
static LOG_FILE: Mutex<Option<fs::File>> = Mutex::new(None);

/// Directory for runtime log files relative to the app data root.
const LOG_SUBDIR: &str = "logs";

/// Initializes the FerrumCraft logger.
///
/// Call this once near the start of `main()`.
pub fn init() -> Result<(), SetLoggerError> {
    let level = match std::env::var("FERRUM_LOG")
        .ok()
        .and_then(|s| s.parse::<Level>().ok())
    {
        Some(l) => l.to_level_filter(),
        None => LevelFilter::Info,
    };

    // Determine the app data directory and create log path.
    let app_dir = crate::storage::app_data_dir();
    let log_dir = app_dir.join(LOG_SUBDIR);
    let _ = fs::create_dir_all(&log_dir);
    let timestamp = chrono_for_filename();
    let log_path = log_dir.join(format!("ferrumcraft_{timestamp}.log"));

    match fs::File::create(&log_path) {
        Ok(file) => {
            let mut guard = LOG_FILE.lock().unwrap();
            *guard = Some(file);
            let _ = writeln!(
                std::io::stdout().lock(),
                "[ INFO] logging: Writing logs to {}",
                log_path.display()
            );
        }
        Err(e) => {
            let _ = writeln!(
                std::io::stdout().lock(),
                "[WARN ] logging: Failed to create log file {}: {e}",
                log_path.display()
            );
        }
    }

    log::set_logger(&LOGGER).map(|()| log::set_max_level(level))
}

struct FerrumLogger;

impl FerrumLogger {
    const fn new() -> Self {
        Self
    }
}

impl Log for FerrumLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = record.level();
        let target = record.target();
        let args = record.args();

        // Format: HH:MM:SS.mmm [LEVEL] target: message
        let ts = timestamp_now();
        let line = format!("{ts} [{level:5}] {target}: {args}\n");

        // Write to stdout.
        let _ = std::io::stdout().lock().write_all(line.as_bytes());

        // Write to file if open.
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        let _ = std::io::stdout().flush();
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(file) = guard.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

/// Returns the current wall clock time as `HH:MM:SS.mmm` for per-line timestamps.
fn timestamp_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let time_secs = total_secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

/// Returns a chrono-style timestamp string for filenames.
fn chrono_for_filename() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Use a simple timestamp format: YYYYMMDD_HHMMSS
    // We manually compute from Unix seconds because we avoid adding a chrono dep.
    // Days since epoch, then compute date/time components.
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // A rough Gregorian date from days since epoch (works from 1970 to 2100).
    let mut y = 1970i64;
    let mut d = days as i64;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if d < year_days {
            break;
        }
        d -= year_days;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days: &[i64] = if leap {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u32;
    for days_in_month in month_days {
        if d < *days_in_month {
            break;
        }
        d -= days_in_month;
        m += 1;
    }
    let day = d as u32 + 1;

    format!("{y:04}{m:02}{day:02}_{hours:02}{minutes:02}{seconds:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
