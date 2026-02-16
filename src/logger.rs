use std::io::Write;

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
        let mut stderr = anstream::stderr();
        let dimmed = anstyle::Style::new().dimmed();
        let _ = writeln!(
            stderr,
            "{dimmed}[{}]{dimmed:#} {}",
            self.category,
            message.as_ref()
        );
    }

    pub fn error(&self, message: impl AsRef<str>) {
        let mut stderr = anstream::stderr();
        let red = anstyle::Style::new()
            .bold()
            .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)));
        let _ = writeln!(
            stderr,
            "{red}[{}]{red:#} {}",
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
