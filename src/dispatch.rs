//! Adaptive agent dispatch — no user flags, no config. The tool decides.

use crate::context::ContextPack;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskComplexity {
    Simple,
    Standard,
    Complex,
}

fn looks_like_path(word: &str) -> bool {
    word.contains('.') && !word.starts_with("http") && word.len() > 3
}

/// Heuristic task classification. Pure function, deterministic, no I/O.
pub fn classify_task(task: &str, pack: &ContextPack) -> TaskComplexity {
    let word_count = task.split_whitespace().count();
    // One file mention is normal for a small fix; several file-like tokens
    // signal multi-file intent and rule out the Simple shortcut.
    let mentioned_files = task
        .split_whitespace()
        .filter(|w| looks_like_path(w))
        .count();
    let file_targets = pack.files.iter().filter(|f| f.complete).count();
    let has_multiple_targets = file_targets > 2;
    let is_large_task = word_count > 120 || has_multiple_targets;

    // Short, single-target, no obvious multi-file intent -> simple. The
    // 8-word bound keeps one-line fixes on the cheap path while pushing
    // multi-step phrasings ("refactor X and add tests") to Standard.
    if word_count <= 8 && file_targets <= 1 && mentioned_files <= 1 {
        TaskComplexity::Simple
    } else if is_large_task {
        TaskComplexity::Complex
    } else {
        TaskComplexity::Standard
    }
}

/// Plan lines that start with an exclusion keyword never contribute targets.
fn is_exclusion_line(line: &str) -> bool {
    let first = line
        .split(|c: char| c.is_whitespace() || c == ',')
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        first.as_str(),
        "ignore" | "ignored" | "skip" | "skipped" | "except" | "excluding" | "leave" | "avoid"
    ) || first == "don't"
        || first == "dont"
}

/// Extract explicit file targets from a plan (lines containing paths from the editable set).
pub fn extract_file_targets_from_plan(
    plan: &str,
    editable: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    let mut targets = std::collections::BTreeSet::new();
    for line in plan.lines() {
        let line = line.trim();
        if is_exclusion_line(line) {
            continue;
        }
        for p in editable.iter() {
            if line.contains(p) && !targets.contains(p) {
                targets.insert(p.clone());
            }
        }
    }
    targets.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextFile, ContextPack};

    fn make_pack(names: &[&str]) -> ContextPack {
        let len = names.len();
        let files = names
            .iter()
            .map(|n| ContextFile {
                path: (*n).into(),
                sha256: "x".into(),
                content: "fn main() {}".into(),
                relevance: 1,
                complete: true,
            })
            .collect();
        ContextPack {
            files,
            total_bytes: len * 100,
            index_file_count: len,
            skipped_secret_like: 0,
        }
    }

    #[test]
    fn classify_simple_task() {
        let pack = make_pack(&["a.rs"]);
        assert_eq!(
            classify_task("fix typo in a.rs comment", &pack),
            TaskComplexity::Simple
        );
    }

    #[test]
    fn classify_standard_task() {
        let pack = make_pack(&["a.rs"]);
        assert_eq!(
            classify_task(
                "refactor a.rs to use iterators instead of loops and add tests",
                &pack
            ),
            TaskComplexity::Standard
        );
    }

    #[test]
    fn classify_complex_multi_file() {
        let pack = make_pack(&["a.rs", "b.rs", "c.rs", "d.rs"]);
        assert_eq!(
            classify_task(
                "implement a new feature that touches a.rs, b.rs, c.rs, and d.rs with new logic",
                &pack
            ),
            TaskComplexity::Complex
        );
    }

    #[test]
    fn classify_simple_multi_word_no_files() {
        let pack = make_pack(&["a.rs"]);
        assert_eq!(
            classify_task("please add a helper function for formatting dates", &pack),
            TaskComplexity::Simple
        );
    }

    #[test]
    fn extract_targets_from_plan() {
        let mut editable = std::collections::BTreeSet::new();
        editable.insert("src/a.rs".into());
        editable.insert("src/b.rs".into());
        editable.insert("tests/integration.rs".into());
        let plan = "Modify src/a.rs to add logging.\nAlso update src/b.rs for consistency.\nIgnore tests/integration.rs for now.";
        let targets = extract_file_targets_from_plan(plan, &editable);
        assert_eq!(targets, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn extract_targets_ordered_by_path() {
        let mut editable = std::collections::BTreeSet::new();
        editable.insert("z.rs".into());
        editable.insert("a.rs".into());
        let plan = "Change z.rs and a.rs";
        let targets = extract_file_targets_from_plan(plan, &editable);
        assert_eq!(targets, vec!["a.rs", "z.rs"]);
    }
}
