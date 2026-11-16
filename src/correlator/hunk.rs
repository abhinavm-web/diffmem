use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: usize,
    pub count: usize,
}

impl LineRange {
    pub fn end(&self) -> usize {
        if self.count == 0 {
            self.start + 1
        } else {
            self.start + self.count
        }
    }

    pub fn overlaps(&self, other: &LineRange, tolerance: usize) -> bool {
        let s1 = self.start.saturating_sub(tolerance);
        let e1 = self.end().saturating_add(tolerance);
        let s2 = other.start.saturating_sub(tolerance);
        let e2 = other.end().saturating_add(tolerance);

        s1 <= e2 && s2 <= e1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLine {
    Context(String),
    Delete(String),
    Add(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub old_range: LineRange,
    pub new_range: LineRange,
    pub header: String,
    pub heading: String,
    pub lines: Vec<DiffLine>,
}

impl Hunk {
    pub fn added_content(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                DiffLine::Add(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn deleted_content(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                DiffLine::Delete(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePatch {
    pub old_path: Option<String>,
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

pub struct UnifiedDiffParser;

impl UnifiedDiffParser {
    pub fn parse(diff_text: &str) -> Vec<FilePatch> {
        let mut patches = Vec::new();
        let mut current_patch: Option<FilePatch> = None;
        let mut current_hunk: Option<Hunk> = None;

        for line in diff_text.lines() {
            if line.starts_with("diff --git ") {
                if let Some(h) = current_hunk.take() {
                    if let Some(p) = &mut current_patch {
                        p.hunks.push(h);
                    }
                }
                if let Some(p) = current_patch.take() {
                    patches.push(p);
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                let new_path = if parts.len() >= 4 {
                    parts[3].trim_start_matches("b/").replace('\\', "/")
                } else {
                    String::new()
                };
                let old_path = if parts.len() >= 3 {
                    Some(parts[2].trim_start_matches("a/").replace('\\', "/"))
                } else {
                    None
                };

                current_patch = Some(FilePatch {
                    old_path,
                    new_path,
                    hunks: Vec::new(),
                });
            } else if let Some(stripped) = line.strip_prefix("--- ") {
                if current_patch.is_none() {
                    let path = stripped.trim().trim_start_matches("a/").replace('\\', "/");
                    current_patch = Some(FilePatch {
                        old_path: Some(path),
                        new_path: String::new(),
                        hunks: Vec::new(),
                    });
                }
            } else if let Some(stripped) = line.strip_prefix("+++ ") {
                let path = stripped.trim().trim_start_matches("b/").replace('\\', "/");
                if let Some(p) = &mut current_patch {
                    p.new_path = path;
                } else {
                    current_patch = Some(FilePatch {
                        old_path: None,
                        new_path: path,
                        hunks: Vec::new(),
                    });
                }
            } else if line.starts_with("@@ ") {
                if let Some(h) = current_hunk.take() {
                    if let Some(p) = &mut current_patch {
                        p.hunks.push(h);
                    }
                }
                if let Some(h) = Self::parse_hunk_header(line) {
                    current_hunk = Some(h);
                }
            } else if let Some(h) = &mut current_hunk {
                if let Some(c) = line.chars().next() {
                    match c {
                        ' ' => h.lines.push(DiffLine::Context(line[1..].to_string())),
                        '-' => h.lines.push(DiffLine::Delete(line[1..].to_string())),
                        '+' => h.lines.push(DiffLine::Add(line[1..].to_string())),
                        _ => {}
                    }
                }
            }
        }

        if let Some(h) = current_hunk.take() {
            if let Some(p) = &mut current_patch {
                p.hunks.push(h);
            }
        }
        if let Some(p) = current_patch.take() {
            patches.push(p);
        }

        patches
    }

    fn parse_hunk_header(line: &str) -> Option<Hunk> {
        let end_idx = line[3..].find("@@")?;
        let inner = line[3..3 + end_idx].trim();
        let heading = line[3 + end_idx + 2..].trim().to_string();

        let parts: Vec<&str> = inner.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let old_range = Self::parse_range(parts[0].trim_start_matches('-'))?;
        let new_range = Self::parse_range(parts[1].trim_start_matches('+'))?;

        Some(Hunk {
            old_range,
            new_range,
            header: line.to_string(),
            heading,
            lines: Vec::new(),
        })
    }

    fn parse_range(spec: &str) -> Option<LineRange> {
        let parts: Vec<&str> = spec.split(',').collect();
        let start = parts[0].parse::<usize>().ok()?;
        let count = if parts.len() > 1 {
            parts[1].parse::<usize>().ok()?
        } else {
            1
        };
        Some(LineRange { start, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_range_overlap() {
        let r1 = LineRange { start: 10, count: 5 }; // [10, 15]
        let r2 = LineRange { start: 14, count: 4 }; // [14, 18]
        assert!(r1.overlaps(&r2, 0));

        let r3 = LineRange { start: 20, count: 5 };
        assert!(!r1.overlaps(&r3, 0));
        assert!(r1.overlaps(&r3, 5)); // within tolerance of 5 lines
    }

    #[test]
    fn test_unified_diff_parser() {
        let diff = r#"diff --git a/src/math.rs b/src/math.rs
index 83db48f..bf269f4 100644
--- a/src/math.rs
+++ b/src/math.rs
@@ -10,3 +10,3 @@ pub fn add(a: i32, b: i32) -> i32 {
-    a - b
+    a + b
     // context line
"#;
        let patches = UnifiedDiffParser::parse(diff);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].new_path, "src/math.rs");
        assert_eq!(patches[0].hunks.len(), 1);
        assert_eq!(patches[0].hunks[0].old_range.start, 10);
        assert_eq!(patches[0].hunks[0].added_content(), vec!["    a + b"]);
        assert_eq!(patches[0].hunks[0].deleted_content(), vec!["    a - b"]);
    }
}
