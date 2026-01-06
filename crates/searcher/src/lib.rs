use matcher::{Matcher, Match};
use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use memmap2::Mmap;

const MMAP_THRESHOLD: u64 = 128 * 1024; // 128 KB
const BUFFER_SIZE: usize = 64 * 1024; // 64 KB

pub struct Searcher <M: Matcher> {
    matcher: M,
}

impl<M: Matcher> Searcher<M> {
    pub fn new(matcher: M) -> Self {
        Searcher { matcher }
    }


    // 1. 添加 should_use_mmap 函数
    fn should_use_mmap(path: &Path) -> Result<bool> {
        let metadata = std::fs::metadata(path)?;
        let file_size = metadata.len();
        Ok(file_size > MMAP_THRESHOLD)
    }

    // 2. 修改 search_file_mmap，处理最后一行
    fn search_file_mmap(&self, path: &Path) -> Result<Vec<Match>> {
        let file = File::open(path)?;
        // SAFETY: 文件在映射期间是只读的，映射的生命周期由 Mmap 管理
        let mmap = unsafe { Mmap::map(&file)? };

        let mut all_matches = Vec::new();
        let mut line_num = 1;
        let mut start = 0;

        for i in 0..mmap.len() {
            if mmap[i] == b'\n' {
                let line_bytes = &mmap[start..i];
                if let Ok(line) = std::str::from_utf8(line_bytes) {
                    let mut matches = self.matcher.find_matches(line);
                    for mat in &mut matches {
                        mat.line = line_num;
                        mat.content = line.to_string();
                    }
                    all_matches.extend(matches);
                }
                start = i + 1;
                line_num += 1;
            }
        }

        // 处理最后一行（如果文件不以换行符结尾）
        if start < mmap.len() {
            let line_bytes = &mmap[start..];
            if let Ok(line) = std::str::from_utf8(line_bytes) {
                let mut matches = self.matcher.find_matches(line);
                for mat in &mut matches {
                    mat.line = line_num;
                    mat.content = line.to_string();
                }
                all_matches.extend(matches);
            }
        }

        Ok(all_matches)
    }

    // 3. 实现块读取的缓冲搜索函数
    fn search_file_buffered(&self, path: &Path) -> Result<Vec<Match>> {
        let file = File::open(path)?;
        let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
        
        let mut all_matches = Vec::new();
        let mut line_num = 1;
        let mut carryover = Vec::new();
        
        // 块读取循环
        loop {
            let mut buffer = vec![0u8; BUFFER_SIZE];
            let bytes_read = reader.read(&mut buffer)?;
            
            if bytes_read == 0 {
                break; // 文件读取完毕
            }
            
            buffer.truncate(bytes_read);
            
            // 处理跨块数据：将 carryover 的内容添加到 buffer 前面
            if !carryover.is_empty() {
                let mut combined = carryover;
                combined.extend_from_slice(&buffer);
                buffer = combined;
                carryover = Vec::new(); // 重新初始化 carryover
            }
            
            // 查找最后一个换行符
            let mut last_newline = None;
            for i in (0..buffer.len()).rev() {
                if buffer[i] == b'\n' {
                    last_newline = Some(i);
                    break;
                }
            }
            
            // 分割完整行和剩余数据
            if let Some(newline_pos) = last_newline {
                let complete_lines = &buffer[..=newline_pos];
                carryover = buffer[newline_pos + 1..].to_vec();
                
                // 处理完整行
                if let Ok(text) = std::str::from_utf8(complete_lines) {
                    for line in text.lines() {
                        let mut matches = self.matcher.find_matches(line);
                        for mat in &mut matches {
                            mat.line = line_num;
                            mat.content = line.to_string();
                        }
                        all_matches.extend(matches);
                        line_num += 1;
                    }
                }
            } else {
                // 没有换行符，整个块是不完整的行
                carryover = buffer;
            }
        }
        
        // 处理文件末尾的剩余数据
        if !carryover.is_empty() {
            if let Ok(line) = std::str::from_utf8(&carryover) {
                let mut matches = self.matcher.find_matches(line);
                for mat in &mut matches {
                    mat.line = line_num;
                    mat.content = line.to_string();
                }
                all_matches.extend(matches);
            }
        }
        
        Ok(all_matches)
    }

    pub fn search_file(&self, path: &Path) -> Result<Vec<Match>> {
        // 根据文件大小选择策略
        if Self::should_use_mmap(path)? {
            self.search_file_mmap(path)
        } else {
            self.search_file_buffered(path)
        }
    }
}