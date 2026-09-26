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
        for (n, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            assert!(
                !trimmed.starts_with("#![allow"),
                "crate/file-level allow at {path}:{line}",
                path = path.display(),
                line = n + 1
            );
        }
        // Every `#[allow(...)]` must name a reason. This file is exempt: it
        // contains the pattern literally.
        if path.file_name().is_some_and(|n| n == "lint_hygiene.rs") {
            continue;
        }
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
