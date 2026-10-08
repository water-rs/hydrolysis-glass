//! The lint policy is structural: no crate- or file-level `#![allow]` — a
//! blanket suppression hides the next violation — and every `#[allow]` on an
//! item carries a `reason`. Checked against the source itself so a new
//! blanket allow fails the suite even if clippy stays silent.

use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn lint_allows_are_item_scoped_with_reasons() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rs_files(&root.join("src"), &mut files);
    rs_files(&root.join("tests"), &mut files);
    for path in &files {
        let text = std::fs::read_to_string(path).expect("readable source");
        // This file is exempt: it contains the patterns literally.
        if path.file_name().is_some_and(|n| n == "lint_hygiene.rs") {
            continue;
        }
        // Scan the whole file text: a multi-line attribute slips past a
        // per-line check.
        let mut rest = text.as_str();
        while let Some(i) = rest.find("#![") {
            let after = &rest[i..];
            assert!(
                !after.starts_with("#![allow") && !after.starts_with("#![expect"),
                "crate/file-level allow/expect at {path}",
                path = path.display()
            );
            if let Some(inner) = after.strip_prefix("#![cfg_attr(") {
                let end = inner.find(")]").expect("cfg_attr attribute closes");
                assert!(
                    !inner[..end].contains("allow(") && !inner[..end].contains("expect("),
                    "crate/file-level allow/expect wrapped in cfg_attr at {path}",
                    path = path.display()
                );
            }
            rest = &after[3..];
        }
        // Every `#[allow(...)]` must name a reason.
        for (i, chunk) in text.split("#[allow(").enumerate().skip(1) {
            let end = chunk.find(")]").expect("allow attribute closes");
            assert!(
                chunk[..end].contains("reason"),
                "allow without a reason at {path} (occurrence {i})",
                path = path.display()
            );
        }
    }
}
