// crates/ignore/src/lib.rs

use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use anyhow::Result;

/// 单个忽略规则
#[derive(Debug, Clone)]
struct Pattern {
    rule: String,           // 原始规则（去除 ! 前缀）
    is_negation: bool,      // 是否为否定规则（!）
    is_directory: bool,    // 是否为目录匹配（以 / 结尾）
}

/// 单个目录的 .gitignore 规则集
#[derive(Debug, Clone)]
struct IgnoreRules {
    patterns: Vec<Pattern>,
    gitignore_dir: PathBuf, // .gitignore 文件所在目录
}

/// 支持嵌套 .gitignore 的忽略系统
pub struct Ignore {
    root: PathBuf,  // 项目根目录
    // 缓存：目录路径 -> 该目录的 .gitignore 规则
    cache: HashMap<PathBuf, IgnoreRules>,
}

impl Ignore {
    /// 创建空的 Ignore 实例
    pub fn new(root: PathBuf) -> Self {
        Ignore {
            root,
            cache: HashMap::new(),
        }
    }
    
    /// 从根目录的 .gitignore 创建实例（向后兼容）
    pub fn from_gitignore(root: &Path) -> Result<Self> {
        let mut ignore = Ignore::new(root.to_path_buf());
        let gitignore_path = root.join(".gitignore");
        if gitignore_path.exists() {
            ignore.load_gitignore_for_dir(root)?;
        }
        Ok(ignore)
    }
    
    /// 为指定目录加载 .gitignore（带缓存）
    fn load_gitignore_for_dir(&mut self, dir: &Path) -> Result<()> {
        // 检查缓存
        if self.cache.contains_key(dir) {
            return Ok(());
        }
        
        let gitignore_path = dir.join(".gitignore");
        let mut patterns = Vec::new();
        
        if gitignore_path.exists() {
            let content = fs::read_to_string(&gitignore_path)?;
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                
                // 解析规则（支持否定规则 !）
                let (is_negation, rule) = if line.starts_with('!') {
                    (true, line[1..].trim().to_string())
                } else {
                    (false, line.to_string())
                };
                
                if !rule.is_empty() {
                    let is_directory = rule.ends_with('/');
                    patterns.push(Pattern {
                        rule,
                        is_negation,
                        is_directory,
                    });
                }
            }
        }
        
        // 存入缓存
        self.cache.insert(
            dir.to_path_buf(),
            IgnoreRules {
                patterns,
                gitignore_dir: dir.to_path_buf(),
            },
        );
        
        Ok(())
    }
    
    /// 查找从根目录到指定路径的所有 .gitignore 文件
    fn find_gitignore_chain(&mut self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut chain = Vec::new();
        
        // 从根目录开始，向上遍历到文件所在目录
        let mut current = if path.is_file() {
            path.parent().unwrap_or(&self.root)
        } else {
            path
        };
        
        // 确保 current 是 root 的子目录或 root 本身
        if !current.starts_with(&self.root) {
            current = &self.root;
        }
        
        // 从文件所在目录向上遍历到根目录
        let mut dirs = Vec::new();
        let mut dir = current;
        loop {
            dirs.push(dir.to_path_buf());
            if dir == &self.root || dir.parent().is_none() {
                break;
            }
            dir = dir.parent().unwrap();
            if !dir.starts_with(&self.root) {
                break;
            }
        }
        
        // 从根目录到文件所在目录的顺序
        dirs.reverse();
        
        // 加载每个目录的 .gitignore
        for dir in &dirs {
            self.load_gitignore_for_dir(dir)?;
            if self.cache.contains_key(dir) {
                chain.push(dir.clone());
            }
        }
        
        Ok(chain)
    }
    
    /// 判断路径是否应该被忽略
    pub fn should_ignore(&mut self, path: &Path) -> bool {
        // 1. 找到所有相关的 .gitignore 文件链
        let gitignore_chain = match self.find_gitignore_chain(path) {
            Ok(chain) => chain,
            Err(_) => return false, // 如果出错，不忽略
        };
        
        // 2. 计算相对于根目录的路径
        let relative_path = match path.strip_prefix(&self.root) {
            Ok(p) => p,
            Err(_) => return false, // 如果路径不在根目录下，不忽略
        };
        
        // 3. 按顺序应用规则（从根目录到子目录）
        let mut should_ignore = false;
        
        for gitignore_dir in gitignore_chain {
            if let Some(rules) = self.cache.get(&gitignore_dir) {
                // 计算相对于该 .gitignore 所在目录的路径
                let dir_relative = match relative_path.strip_prefix(
                    gitignore_dir.strip_prefix(&self.root).unwrap_or(&gitignore_dir)
                ) {
                    Ok(p) => p,
                    Err(_) => continue, // 如果路径不在该目录下，跳过
                };
                
                let dir_relative_str = dir_relative.to_string_lossy().replace('\\', "/");
                
                // 应用该目录的所有规则
                for pattern in &rules.patterns {
                    if self.match_pattern(&pattern.rule, &dir_relative_str, pattern.is_directory) {
                        if pattern.is_negation {
                            // 否定规则：取消忽略
                            should_ignore = false;
                        } else {
                            // 普通规则：标记为忽略
                            should_ignore = true;
                        }
                    }
                }
            }
        }
        
        should_ignore
    }
    
    /// 匹配单个规则（使用相对路径）
    fn match_pattern(&self, pattern: &str, relative_path: &str, is_directory: bool) -> bool {
        // 1️⃣ 完全匹配
        if pattern == relative_path {
            return true;
        }
        
        // 2️⃣ 目录匹配（pattern 以 / 结尾）
        if is_directory {
            let dir_pattern = pattern.trim_end_matches('/');
            // 匹配路径中包含该目录的情况
            if relative_path.contains(&format!("{}/", dir_pattern)) {
                return true;
            }
            // 匹配路径以该目录开头的情况
            if relative_path.starts_with(dir_pattern) {
                return true;
            }
            return false;
        }
        
        // 3️⃣ 通配符匹配（简化版）
        if pattern.contains('*') {
            // "*" 匹配任意文件名（不跨目录）
            if pattern == "*" {
                return !relative_path.contains('/');
            }
            
            // "*.ext"
            if let Some(ext) = pattern.strip_prefix("*.") {
                return relative_path.ends_with(ext);
            }
            
            // "prefix*"
            if let Some(prefix) = pattern.strip_suffix('*') {
                return relative_path.starts_with(prefix);
            }
            
            // 其他复杂情况，基础版不支持
            return false;
        }
        
        // 4️⃣ 文件名匹配（pattern 不包含 /）
        if !pattern.contains('/') {
            if let Some(name) = Path::new(relative_path).file_name() {
                if let Some(name_str) = name.to_str() {
                    return name_str == pattern;
                }
            }
            return false;
        }
        
        // 5️⃣ 路径匹配（pattern 包含 /）
        relative_path.contains(pattern)
    }
}