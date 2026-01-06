pub use run_app as run; 
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;
use std::path::{Path, PathBuf};
use clap::Parser;
use matcher::RegexMatcher;
use searcher::Searcher;
use printer::Printer;
use anyhow::{Context, Result, bail};
use ignore::Ignore;
use rayon::prelude::*;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[arg(help = "The regex pattern to search for")]
    pattern: String,

    #[arg(help = "Files or directories to search", default_value = ".")]
    paths: Vec<PathBuf>,

    /// Number of threads to use for parallel search (0 = auto-detect, 1 = single-threaded)
    #[arg(long, short = 'j', default_value = "0", help = "Number of threads (0 = auto, 1 = single-threaded)")]
    jobs: usize,
}

pub fn run_app() -> Result<()> {
    let args = Args::parse();

    let matcher = RegexMatcher::new(&args.pattern)
        .context(format!("Invalid regex pattern: '{}'", args.pattern))?;

    let searcher = Arc::new(Searcher::new(matcher));
    let printer = Arc::new(Mutex::new(Printer::new()));
    
    // 如果指定了 jobs > 1，设置 rayon 的线程池
    if args.jobs > 1 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.jobs)
            .build_global()
            .unwrap();
    }

    // jobs == 1 表示单线程，jobs == 0 或 jobs > 1 表示并行
    let use_parallel = args.jobs != 1;
    process_paths(searcher.clone(), printer.clone(), &args.paths, use_parallel)
}

fn process_paths(
    searcher: Arc<Searcher<RegexMatcher>>, 
    printer: Arc<Mutex<Printer>>, 
    paths: &[PathBuf],
    use_parallel: bool  // 添加参数
) -> Result<()> {
    for path in paths {
        handle_single_path(searcher.clone(), printer.clone(), path, use_parallel)?;
    }
    Ok(())
}


fn handle_single_path(
    searcher: Arc<Searcher<RegexMatcher>>,
    printer: Arc<Mutex<Printer>>,
    path: &Path,
    use_parallel: bool,
) -> Result<()> {
    if !path.exists() {
        bail!("File or directory not found: {}", path.display());
    }

    // 确定根目录
    let root = if path.is_file() {
        path.parent().unwrap_or_else(|| Path::new("."))
    } else {
        path
    };
    
    // 创建 Ignore 实例（使用根目录）
    let ignore = Ignore::from_gitignore(root).unwrap_or_else(|_| Ignore::new(root.to_path_buf()));
    let ignore_arc = Arc::new(Mutex::new(ignore));

    if path.is_file() {
        {
            let mut ignore_guard = ignore_arc.lock().unwrap();
            if ignore_guard.should_ignore(path) {
                // 文件被 .gitignore 忽略，静默跳过（符合 ripgrep 行为）
                return Ok(());
            }
        }
        // 对于单个文件，使用单线程版本
        let printer_guard = printer.lock().unwrap();
        search_file_and_print(&*searcher, &*printer_guard, path)?;
        return Ok(());
    }

    if path.is_dir() {
        // 根据参数决定使用并行还是单线程版本
        if use_parallel {
            walk_directory_parallel(searcher, printer, path, ignore_arc)?;
        } else {
            walk_directory_single_thread(searcher, printer, path, ignore_arc)?;
        }
    }

    Ok(())
}

/// 单线程版本的目录遍历函数
fn walk_directory_single_thread(
    searcher: Arc<Searcher<RegexMatcher>>,
    printer: Arc<Mutex<Printer>>,
    dir_path: &Path,
    ignore: Arc<Mutex<Ignore>>,
) -> Result<()> {
    let walk_dir = WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter();
    
    for entry_result in walk_dir {
        let entry = entry_result?;
        let path = entry.path();
        
        // 显式跳过 .git 目录及其所有子项
        let path_str = path.to_string_lossy();
        if path_str.contains(".git/") || path_str.contains(".git\\") {
            continue;
        }
        
        if entry.file_type().is_file() {
            // 检查是否被忽略
            {
                if let Ok(mut ignore_guard) = ignore.lock() {
                    if ignore_guard.should_ignore(path) {
                        continue;
                    }
                }
            }
            
            // 搜索文件
            let matches = match searcher.search_file(path) {
                Ok(matches) => matches,
                Err(_) => continue, // 跳过无法读取的文件
            };
            
            // 打印结果
            if let Ok(printer_guard) = printer.lock() {
                for mat in matches {
                    let _ = printer_guard.print_match(path, &mat);
                }
            }
        }
    }
    Ok(())
}



fn walk_directory_parallel(
    searcher: Arc<Searcher<RegexMatcher>>,
    printer: Arc<Mutex<Printer>>,
    dir_path: &Path,
    ignore: Arc<Mutex<Ignore>>
) -> Result<()> {

    // 1️⃣ 收集所有需要处理的文件路径（串行）
    // 注意：在收集阶段也需要检查 .gitignore，所以需要获取锁
    let files: Vec<PathBuf> = WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;          // 跳过 WalkDir 错误
            let path = entry.path();

            // 跳过 .git 目录及其子项
            let path_str = path.to_string_lossy();
            if path_str.contains(".git/") || path_str.contains(".git\\") {
                return None;
            }

            // 只处理普通文件
            if !entry.file_type().is_file() {
                return None;
            }

            // .gitignore 过滤（需要获取锁，但尽量减少锁的持有时间）
            {
                if let Ok(mut ignore_guard) = ignore.lock() {
                    if ignore_guard.should_ignore(path) {
                        return None;
                    }
                }
            }

            Some(path.to_path_buf())
        })
        .collect();

    // 2️⃣ 并行搜索文件
    // 注意：文件已经在收集阶段过滤过了，并行处理时不需要再检查 .gitignore
    files.par_iter()
        .for_each(|path| {
            // 搜索文件
            let matches = match searcher.search_file(path) {
                Ok(matches) => matches,
                Err(_) => return, // 跳过无法读取的文件
            };
            
            // 获取锁后打印结果
            if let Ok(printer_guard) = printer.lock() {
                for mat in matches {
                    let _ = printer_guard.print_match(path, &mat);
                }
            }
        });
    
    Ok(())
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