use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::Write;

#[derive(Debug)]
pub struct CliError {
    message: String,
    hint: Option<String>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl CliError {
    pub fn with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: Some(hint.into()),
            source: None,
        }
    }

    pub fn with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            hint: None,
            source: Some(Box::new(source)),
        }
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|err| err.as_ref() as &(dyn Error + 'static))
    }
}

pub fn exit_code(_: &CliError) -> i32 {
    1
}

pub fn print_error(error: &CliError, verbose: bool) {
    let mut stderr = anstream::stderr();
    let red = anstyle::Style::new()
        .bold()
        .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red)));
    let cyan = anstyle::Style::new()
        .bold()
        .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Cyan)));
    let dimmed = anstyle::Style::new().dimmed();

    let _ = writeln!(stderr, "{red}error:{red:#} {error}");

    if let Some(hint) = &error.hint {
        let _ = writeln!(stderr, "  {cyan}hint:{cyan:#} {dimmed}{hint}{dimmed:#}");
    }

    if verbose {
        let mut source = error.source();
        while let Some(cause) = source {
            let _ = writeln!(stderr, "  {dimmed}caused by:{dimmed:#} {cause}");
            source = cause.source();
        }
    }
}
