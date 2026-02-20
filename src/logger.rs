use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct Logger {
    category: String,
}

impl Logger {
    pub fn child(&self, name: &str) -> Self {
        Self {
            category: format!("{}.{}", self.category, name),
        }
    }

    pub fn info(&self, message: impl AsRef<str>) {
        let timestamp = current_timestamp();
        let mut stderr = anstream::stderr();
        let dimmed = anstyle::Style::new().dimmed();
        let _ = writeln!(
            stderr,
            "{dimmed}[{timestamp}] [{}]{dimmed:#} {}",
            self.category,
            message.as_ref()
        );
    }

    pub fn error(&self, message: impl AsRef<str>) {
        let timestamp = current_timestamp();
        let mut stderr = anstream::stderr();
        let red = anstyle::Style::new()
            .bold()
            .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)));
        let _ = writeln!(
            stderr,
            "{red}[{timestamp}] [{}]{red:#} {}",
            self.category,
            message.as_ref()
        );
    }
}

pub fn get_logger() -> Logger {
    Logger {
        category: "work".to_string(),
    }
}

fn current_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format_unix_timestamp(secs)
}

fn format_unix_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;

    // Civil date from days since epoch (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::format_unix_timestamp;

    #[test]
    fn format_unix_timestamp_formats_epoch() {
        assert_eq!(format_unix_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_unix_timestamp_formats_recent_time() {
        assert_eq!(format_unix_timestamp(1_771_606_800), "2026-02-20T17:00:00Z");
    }

    #[test]
    fn format_unix_timestamp_handles_leap_day() {
        assert_eq!(format_unix_timestamp(1_709_251_199), "2024-02-29T23:59:59Z");
    }
}
