use regex::Regex;
use anyhow::Result;

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

pub struct RegexMatcher {
    regex: Regex,
}

impl RegexMatcher {
    pub fn new(pattern: &str) -> Result<Self> {
        Ok(Self {
            regex: Regex::new(pattern)?,
        })
    }
}

impl Matcher for RegexMatcher {
    fn find_matches(&self, haystack: &str) -> Vec<Match> {
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
        self.regex.is_match(haystack)
    }
}
