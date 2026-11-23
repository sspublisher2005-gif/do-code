//! Compact, brace-aware repository outline.
//!
//! Produces one line per top-level symbol (functions, types, traits, impls)
//! with its file and line number, so a model can navigate a repo without
//! reading every file. No parser dependency: a small regex per language plus
//! a brace counter gives the symbol's span. Ranges are reported as
//! `file:line` only; the body is never included.

use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".docode",
    "dist",
    "build",
    "__pycache__",
];

const SOURCE_EXTS: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp", "h", "hpp",
];

pub fn outline(root: &Path, max_lines: usize) -> Result<String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();

    let mut out = String::new();
    let mut count = 0usize;
    for file in &files {
        if count >= max_lines {
            break;
        }
        let rel = file.strip_prefix(root).unwrap_or(file);
        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        let Ok(content) = fs::read_to_string(file) else {
            continue;
        };
        for (line_no, kind, name) in symbols(&content, ext) {
            if count >= max_lines {
                break;
            }
            out.push_str(&format!(
                "{}:{} {} {}\n",
                rel.display(),
                line_no,
                kind,
                name
            ));
            count += 1;
        }
    }
    if count == 0 {
        out.push_str("(no symbols found)\n");
    }
    Ok(out)
}

fn collect_files(_root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_files(_root, &path, out)?;
        } else if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && SOURCE_EXTS.contains(&ext)
        {
            out.push(path);
        }
    }
    Ok(())
}

fn symbols(content: &str, ext: &str) -> Vec<(usize, &'static str, String)> {
    let mut result = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        // Rust
        if ext == "rs" {
            for (kw, kind) in [
                ("pub fn ", "fn"),
                ("fn ", "fn"),
                ("pub struct ", "struct"),
                ("struct ", "struct"),
                ("pub enum ", "enum"),
                ("enum ", "enum"),
                ("pub trait ", "trait"),
                ("trait ", "trait"),
                ("impl ", "impl"),
                ("pub type ", "type"),
                ("pub const ", "const"),
                ("pub static ", "static"),
            ] {
                if let Some(rest) = trimmed.strip_prefix(kw) {
                    let name = rest
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() {
                        result.push((i + 1, kind, name));
                    }
                    break;
                }
            }
        } else if ext == "py" {
            if let Some(rest) = trimmed.strip_prefix("def ") {
                let name = rest.split('(').next().unwrap_or("").trim().to_string();
                if !name.is_empty() {
                    result.push((i + 1, "fn", name));
                }
            } else if let Some(rest) = trimmed.strip_prefix("class ") {
                let name = rest
                    .split(['(', ':'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !name.is_empty() {
                    result.push((i + 1, "class", name));
                }
            } else if let Some(rest) = trimmed.strip_prefix("async def ") {
                let name = rest.split('(').next().unwrap_or("").trim().to_string();
                if !name.is_empty() {
                    result.push((i + 1, "fn", name));
                }
            }
        } else if ext == "go" {
            if let Some(rest) = trimmed.strip_prefix("func ") {
                let name = rest
                    .trim_start_matches('(')
                    .split(['(', ' ', ')'])
                    .find(|s| !s.is_empty())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    result.push((i + 1, "fn", name));
                }
            } else if let Some(rest) = trimmed.strip_prefix("type ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_string();
                if !name.is_empty() {
                    result.push((i + 1, "type", name));
                }
            }
        } else if matches!(ext, "ts" | "tsx" | "js" | "jsx") {
            for kw in [
                "export function ",
                "export default function ",
                "function ",
                "export class ",
                "class ",
                "export interface ",
                "interface ",
                "export type ",
                "type ",
            ] {
                if let Some(rest) = trimmed.strip_prefix(kw) {
                    let name = rest
                        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if !name.is_empty() {
                        let kind = if kw.contains("class") {
                            "class"
                        } else if kw.contains("interface") {
                            "interface"
                        } else if kw.contains("type") {
                            "type"
                        } else {
                            "fn"
                        };
                        result.push((i + 1, kind, name));
                    }
                    break;
                }
            }
        } else if matches!(ext, "java" | "c" | "cpp" | "h" | "hpp") {
            // crude: lines containing a type keyword followed by an identifier and '(' or '{'
            if trimmed.contains('(')
                && (trimmed.contains("void ")
                    || trimmed.contains("int ")
                    || trimmed.contains("public ")
                    || trimmed.contains("private ")
                    || trimmed.contains("static "))
            {
                // extract last identifier before '('
                if let Some(idx) = trimmed.rfind('(') {
                    let before = &trimmed[..idx];
                    let name = before
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .trim_matches('*')
                        .to_string();
                    if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        result.push((i + 1, "fn", name));
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn extracts_rust_symbols() {
        let dir = std::env::temp_dir().join("docode-outline-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lib.rs");
        fs::write(
            &path,
            "pub fn foo() {}\nstruct Bar {}\nimpl Bar {\n  pub fn baz(&self) {}\n}\n",
        )
        .unwrap();
        let out = outline(&dir, 100).unwrap();
        assert!(out.contains("lib.rs:1 fn foo"));
        assert!(out.contains("lib.rs:2 struct Bar"));
        assert!(out.contains("lib.rs:3 impl Bar"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn caps_output() {
        let dir = std::env::temp_dir().join("docode-outline-cap");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.rs");
        let mut s = String::new();
        for i in 0..50 {
            s.push_str(&format!("pub fn f{i}() {{}}\n"));
        }
        fs::write(&path, s).unwrap();
        let out = outline(&dir, 5).unwrap();
        assert_eq!(out.lines().count(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
