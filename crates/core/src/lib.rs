pub use run_app as run; 
use walkdir::WalkDir;
use std::fs;
use std::path::{Path, PathBuf};
use clap::Parser;
use matcher::RegexMatcher;
use searcher::Searcher;
use printer::Printer;
use anyhow::{Context, Result, bail};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(help = "The regex pattern to search for")]
    pattern: String,

    #[arg(help = "Files or directories to search", default_value = ".")]
    paths: Vec<PathBuf>,
}

pub fn run_app() -> Result<()> {
    let args = Args::parse();

    let matcher = RegexMatcher::new(&args.pattern)
        .context(format!("Invalid regex pattern: '{}'", args.pattern))?;

    let searcher = Searcher::new(matcher);
    
    let printer = Printer::new();

    process_paths(&searcher, &printer, &args.paths)
}

fn process_paths(
    searcher: &Searcher<RegexMatcher>, 
    printer: &Printer, 
    paths: &[PathBuf]
) -> Result<()> {
    for path in paths {
        handle_single_path(searcher, printer, path)?;
    }
    Ok(())
}

fn handle_single_path(
    searcher: &Searcher<RegexMatcher>, 
    printer: &Printer, 
    path: &Path
) -> Result<()> {
    if !path.exists() {
        bail!("File or directory not found: {}", path.display());
    }

    if path.is_dir() {
        eprintln!("Info: Directory searching is not supported in Phase 1: {}", path.display());
        walk_directory(searcher, printer, path)?;
        return Ok(());
    } else if path.is_file() {
        search_file_and_print(searcher, printer, path)?;
    }

    Ok(())
}

fn walk_directory(searcher: &Searcher<RegexMatcher>, printer: &Printer, dir_path: &Path) -> Result<()>{
    let walk_dir = WalkDir::new(dir_path).follow_links(false);
    walk_dir.into_iter().
}

fn search_file_and_print(
    searcher: &Searcher<RegexMatcher>, 
    printer: &Printer, 
    path: &Path
) -> Result<()> {
    let matches = searcher.search_file(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    for mat in matches {
        printer.print_match(path, &mat)?;
    }
    
    Ok(())
}