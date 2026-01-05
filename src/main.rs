use core::run;
use std::process;

fn main() {
    if let Err(e) = run() {
        eprintln!("错误: {}", e);
        process::exit(1);
    }
}

