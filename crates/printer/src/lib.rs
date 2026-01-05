use std::io::{self, Write};
use std::path::Path;
use matcher::Match;

pub struct Printer {
    output: Box<dyn Write>
}

impl Printer {
    pub fn new() -> Self {
        Printer { output: Box::new(io::stdout()) }
    }

    pub fn print_match(&self, path: &Path, m: &Match) -> io::Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();

        writeln!(
            handle,
            "{}:{}:{}",
            path.display(),
            m.line,
            m.content
        )?;
        Ok(())
    }
}