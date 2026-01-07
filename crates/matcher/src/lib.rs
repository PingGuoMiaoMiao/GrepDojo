use regex::Regex;
use anyhow::Result;
use memchr::memmem::Finder;
use memchr;
use std::collections::HashMap;

pub struct Match {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub content: String,
}

impl Match {
    pub fn new(start: usize, end: usize, line: usize, content: String) -> Self {
        Self {
            start,
            end,
            line,
            content,
        }
    }
}

pub trait Matcher {
    fn find_matches(&self, haystack: &str) -> Vec<Match>;
    fn is_match(&self, haystack: &str) -> bool;
}

// 字面量提取辅助函数
fn is_special_char(c: char) -> bool {
    matches!(c, '^' | '$' | '.' | '*' | '+' | '?' | '{' | '}' | '[' | ']' | '(' | ')' | '|' | '\\')
}

// 稀有字节选择辅助函数
fn is_special_byte(b: u8) -> bool {
    matches!(b, b'.' | b'*' | b'+' | b'?' | b'{' | b'}' | b'[' | b']' | b'(' | b')' | b'|' | b'\\' | b'^' | b'$')
}

fn select_rare_byte(pattern: &str) -> Option<u8> {
    // 1. 提取字面量字节
    let bytes: Vec<u8> = pattern
        .bytes()
        .filter(|&b| !is_special_byte(b))
        .collect();
    
    if bytes.is_empty() {
        return None;
    }
    
    // 2. 统计频率
    let mut freq: HashMap<u8, usize> = HashMap::new();
    for &b in &bytes {
        *freq.entry(b).or_insert(0) += 1;
    }
    
    // 3. 选择最稀有的字节
    let rare_byte = freq.iter()
        .min_by_key(|(_, count)| *count)
        .map(|(&byte, _)| byte);
    
    // 4. 检查是否足够稀有（频率 <= 5）
    if let Some(byte) = rare_byte {
        if freq[&byte] <= 5 {
            return Some(byte);
        }
    }
    
    None
}

fn is_pure_literal(pattern: &str) -> bool {
    !pattern.chars().any(is_special_char)
}

fn extract_prefix(pattern: &str) -> String {
    let mut prefix = String::new();
    let mut chars = pattern.chars().peekable();
    
    while let Some(c) = chars.peek() {
        match c {
            '.' | '*' | '+' | '?' | '{' | '[' | '(' | '|' => break,
            '\\' => {
                // 处理转义字符
                chars.next();
                if let Some(escaped) = chars.next() {
                    prefix.push(escaped);
                }
            }
            _ => {
                prefix.push(*c);
                chars.next();
            }
        }
    }
    
    prefix
}

fn extract_literals(pattern: &str) -> Option<String> {
    // 1. 检查是否为纯字面量
    if is_pure_literal(pattern) {
        return Some(pattern.to_string());
    }
    
    // 2. 提取固定前缀
    let prefix = extract_prefix(pattern);
    if prefix.len() >= 3 {
        return Some(prefix);
    }
    
    // 3. 无法提取字面量
    None
}

pub struct RegexMatcher {
    regex: Regex,
    literal: Option<String>,
    literal_finder: Option<Finder<'static>>,
    rare_byte: Option<u8>,
}

impl RegexMatcher {
    pub fn new(pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern)?;
        
        // 提取字面量
        let literal = extract_literals(pattern);
        let literal_finder = literal.as_ref().map(|lit| {
            // 使用 Box::leak 将字面量转换为 'static 生命周期
            let leaked = Box::leak(lit.clone().into_boxed_str());
            Finder::new(leaked.as_bytes())
        });
        
        // 选择稀有字节（如果没有字面量，或者作为补充优化）
        let rare_byte = select_rare_byte(pattern);
        
        Ok(Self {
            regex,
            literal,
            literal_finder,
            rare_byte,
        })
    }
    
    // 使用稀有字节跳过的辅助方法
    fn find_matches_with_rare_byte(&self, haystack: &str, rare_byte: u8) -> Vec<Match> {
        let mut matches = Vec::new();
        let mut pos = 0;
        let window_size = 200; // 固定窗口大小
        
        // 搜索稀有字节
        while let Some(byte_pos) = memchr::memchr(rare_byte, haystack[pos..].as_bytes()) {
            let candidate_pos = pos + byte_pos;
            
            // 提取候选位置周围的文本（滑动窗口）
            let start = candidate_pos.saturating_sub(window_size);
            let end = (candidate_pos + window_size).min(haystack.len());
            let candidate = &haystack[start..end];
            
            // 使用正则验证
            for mat in self.regex.find_iter(candidate) {
                let adjusted_start = start + mat.start();
                let adjusted_end = start + mat.end();
                
                matches.push(Match::new(
                    adjusted_start,
                    adjusted_end,
                    0, // line will be filled by Searcher
                    mat.as_str().to_string(),
                ));
            }
            
            pos = candidate_pos + 1;
        }
        
        // 去重：按位置排序并去重
        matches.sort_by_key(|m| (m.start, m.end));
        matches.dedup_by(|a, b| a.start == b.start && a.end == b.end);
        
        matches
    }
}

impl Matcher for RegexMatcher {
    fn find_matches(&self, haystack: &str) -> Vec<Match> {
        // 1. 如果有字面量，使用字面量预过滤
        if let Some(ref finder) = self.literal_finder {
            // 使用字面量预过滤：先检查字面量是否存在
            // 因为 Searcher 已经逐行处理，如果字面量存在，验证整行
            if finder.find_iter(haystack.as_bytes()).next().is_some() {
                // 字面量存在，验证整行是否匹配正则
                return self.regex.find_iter(haystack).map(|mat| {
                    Match::new(
                        mat.start(),
                        mat.end(),
                        0, // line will be filled by Searcher
                        mat.as_str().to_string(),
                    )
                }).collect();
            } else {
                // 字面量不存在，直接返回空结果（快速跳过）
                return Vec::new();
            }
        }
        
        // 2. 如果没有字面量，尝试使用稀有字节跳过
        if let Some(rare_byte) = self.rare_byte {
            return self.find_matches_with_rare_byte(haystack, rare_byte);
        }
        
        // 3. 既没有字面量也没有稀有字节，直接使用正则
        self.regex.find_iter(haystack).map(|mat| {
            Match::new(
                mat.start(),
                mat.end(),
                0, // line will be filled by Searcher
                mat.as_str().to_string(),
            )
        }).collect()
    }

    fn is_match(&self, haystack: &str) -> bool {
        // 1. 如果有字面量，先检查字面量是否存在
        if let Some(ref finder) = self.literal_finder {
            if finder.find_iter(haystack.as_bytes()).next().is_some() {
                // 字面量存在，使用正则验证
                return self.regex.is_match(haystack);
            } else {
                // 字面量不存在，直接返回 false
                return false;
            }
        }
        
        // 2. 如果没有字面量，尝试使用稀有字节跳过
        if let Some(rare_byte) = self.rare_byte {
            // 检查稀有字节是否存在
            if memchr::memchr(rare_byte, haystack.as_bytes()).is_some() {
                // 稀有字节存在，使用正则验证
                return self.regex.is_match(haystack);
            } else {
                // 稀有字节不存在，直接返回 false
                return false;
            }
        }
        
        // 3. 既没有字面量也没有稀有字节，直接使用正则
        self.regex.is_match(haystack)
    }
}
