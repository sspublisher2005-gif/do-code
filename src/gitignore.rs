//! Minimal, deterministic .gitignore matching used by Do Code's file scanner.
//!
//! This is a conservative implementation of git's ignore semantics for the
//! subset that matters to repository scanning:
//!   - blank lines and `#` comments are ignored,
//!   - `dir/` marks directory-only patterns,
//!   - a leading `/` anchors the pattern to the directory of the .gitignore
//!     file it appears in,
//!   - `*` matches within a path segment, `**` crosses segments,
//!   - a `!` prefix re-includes (last matching rule wins),
//!   - rules are evaluated with the last matching rule taking precedence.
//!
//! It intentionally does NOT implement git's full glob edge cases (character
//! classes, escaped metacharacters) — those are rare in real repositories, and
//! Do Code's sensitive-path skip list remains a hard floor regardless of this
//! matcher. Treat this as a best-effort parity aid, not a git clone.

#[derive(Clone, Debug)]
struct Rule {
    /// Pattern with leading `!`, trailing `/` and leading `/` stripped.
    pattern: String,
    /// True when the rule was `dir/`-style (matches directories only).
    dir_only: bool,
    /// True when the pattern was anchored with a leading `/`.
    anchored: bool,
    /// True when the pattern was negated with `!`.
    negated: bool,
}

/// A set of ignore rules accumulated from one or more .gitignore files.
#[derive(Clone, Debug, Default)]
pub struct GitignoreSet {
    rules: Vec<Rule>,
}

impl GitignoreSet {
    /// Parse the contents of a .gitignore file into rules.
    pub fn parse(&mut self, contents: &str) {
        for raw in contents.lines() {
            let line = raw.trim_end();
            // Trailing spaces (before CRLF) are not significant.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut negated = false;
            let mut pattern = line;
            if let Some(stripped) = pattern.strip_prefix('!') {
                negated = true;
                pattern = stripped;
            }
            // A pattern that is only "!" or empty after stripping is invalid.
            if pattern.trim().is_empty() {
                continue;
            }
            let mut dir_only = false;
            if let Some(stripped) = pattern.strip_suffix('/') {
                dir_only = true;
                pattern = stripped;
            }
            let mut anchored = false;
            if let Some(stripped) = pattern.strip_prefix('/') {
                anchored = true;
                pattern = stripped;
            }
            if pattern.is_empty() {
                continue;
            }
            self.rules.push(Rule {
                pattern: pattern.to_owned(),
                dir_only,
                anchored,
                negated,
            });
        }
    }

    /// Returns true when `rel_path` (relative to the .gitignore root, using
    /// `/` separators) is ignored by these rules. `is_dir` matters for
    /// directory-only rules. Last matching rule wins (git semantics).
    pub fn is_ignored(&self, rel_path: &str, is_dir: bool) -> bool {
        let mut ignored = false;
        for rule in &self.rules {
            if rule.dir_only && !is_dir {
                continue;
            }
            if rule.anchored {
                // Anchored patterns match only against the top-level path.
                if glob_matches(&rule.pattern, rel_path) {
                    ignored = !rule.negated;
                }
            } else if glob_matches_nested(&rule.pattern, rel_path) {
                ignored = !rule.negated;
            }
        }
        ignored
    }

    /// Total number of parsed rules (useful for tests/diagnostics).
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether no rules were loaded.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Match a pattern (no `**`) against a single path segment list.
/// `*` matches within a segment; `?` matches one char.
fn glob_matches(pattern: &str, path: &str) -> bool {
    if pattern.contains("**") {
        return glob_matches_nested(pattern, path);
    }
    let pat_parts: Vec<&str> = pattern.split('/').filter(|p| !p.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if pat_parts.len() != path_parts.len() {
        return false;
    }
    pat_parts
        .iter()
        .zip(path_parts.iter())
        .all(|(p, s)| segment_matches(p, s))
}

/// Match a pattern that may contain `**` against a full relative path.
fn glob_matches_nested(pattern: &str, path: &str) -> bool {
    if !pattern.contains("**") {
        // A non-`**` unanchored pattern matches any suffix of the path.
        if pattern.contains('/') {
            return glob_matches(pattern, path);
        }
        // Single-segment pattern: match any segment at any nesting.
        let path_parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        return path_parts.iter().any(|part| segment_matches(pattern, part));
    }
    // `**` handling: split on `**` and require each piece to match in order.
    let pieces: Vec<&str> = pattern.split("**").collect();
    let path_bytes = path.as_bytes();
    let mut search_from = 0_usize;
    for (i, piece) in pieces.iter().enumerate() {
        if piece.is_empty() {
            continue;
        }
        // Find `piece` starting at or after search_from; the first piece must
        // match at the start, the last at the end.
        let rel = &path[search_from..];
        match find_segment_suffix(rel, piece) {
            Some(idx) => {
                let abs = search_from + idx;
                if i == 0 && !path.starts_with(piece) && pieces.len() > 1 {
                    // First piece must be a prefix (unless pattern starts with **).
                    return false;
                }
                search_from = abs + piece.len();
            }
            None => return false,
        }
        let _ = path_bytes;
    }
    true
}

fn find_segment_suffix(text: &str, needle: &str) -> Option<usize> {
    text.find(needle)
}

/// Match a single path segment against a `*`/`?` pattern (no `/`).
fn segment_matches(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let pat: Vec<char> = pattern.chars().collect();
    let seg: Vec<char> = segment.chars().collect();
    // DP wildcard match over the two char sequences.
    let n = pat.len();
    let m = seg.len();
    let mut dp = vec![vec![false; m + 1]; n + 1];
    dp[0][0] = true;
    for i in 1..=n {
        if pat[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=n {
        for j in 1..=m {
            if pat[i - 1] == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if pat[i - 1] == '?' || pat[i - 1] == seg[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }
    dp[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_and_ignores_matching() {
        let mut set = GitignoreSet::default();
        set.parse("target/\n*.log\n!important.log\n/build\nnode_modules/\n");
        assert_eq!(set.len(), 5);
        assert!(set.is_ignored("target", true));
        assert!(!set.is_ignored("target", false)); // dir-only rule
        assert!(set.is_ignored("debug.log", false));
        assert!(!set.is_ignored("important.log", false)); // negation wins
        assert!(set.is_ignored("build", true));
        assert!(set.is_ignored("node_modules/pkg", true));
    }

    #[test]
    fn nested_glob_and_single_segment_rules() {
        let mut set = GitignoreSet::default();
        set.parse("**/generated/**\n*.tmp\n");
        assert!(set.is_ignored("a/b/generated/c.rs", false));
        assert!(set.is_ignored("run.tmp", false));
        assert!(!set.is_ignored("src/main.rs", false));
    }

    #[test]
    fn anchored_rule_matches_top_level_only() {
        let mut set = GitignoreSet::default();
        set.parse("/output\n");
        assert!(set.is_ignored("output", true));
        assert!(!set.is_ignored("sub/output", true));
    }
}
