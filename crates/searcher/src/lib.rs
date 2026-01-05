use matcher::{Matcher, Match};
use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, BufRead};
use std::path::Path;

pub struct Searcher <M: Matcher> {
    matcher: M,
}

impl<M: Matcher> Searcher<M> {
    pub fn new(matcher: M) -> Self {
        Searcher { matcher }
    }

    pub fn search_file(&self, path: &Path) -> Result<Vec<Match>>{
        let file = File::open(path)?;
        
        let reader = BufReader::new(file);
        
        let mut all_matches = Vec::new();

        for (i, line_result) in reader.lines().enumerate() {
            let line = line_result?;
            let line_num = i + 1;
            let mut matches = self.matcher.find_matches(&line);
            for mat in &mut matches {
                mat.line = line_num;
                mat.content = line.clone(); // Set full line content
            }
            all_matches.extend(matches);
        }
        Ok(all_matches)
    }
}