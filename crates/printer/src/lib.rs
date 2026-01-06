use std::io::{self, Write};
use std::path::Path;
use matcher::Match;

pub struct Printer {
    // 移除 output 字段，因为 print_match 中直接使用 stdout()
    // 这样可以避免 Box<dyn Write> 的 Send 问题
}

impl Printer {
    pub fn new() -> Self {
        Printer {}
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