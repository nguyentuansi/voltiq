//! Walk a directory for scannable text files, honouring `.gitignore` and skipping
//! dependency/build directories (build outputs are scanned separately as client
//! bundles in the client-bundle surface).

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// Directories never treated as *source* (deps + build artifacts + VCS).
pub const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "out",
    ".next",
    ".svelte-kit",
    "target",
    ".turbo",
    "vendor",
    "coverage",
    ".cache",
];

/// Files larger than this are skipped (a real secret is small; large files are
/// usually data/minified assets).
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// Collect candidate source files under `root`.
pub fn walk_source_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // `hidden(false)` so dotfiles like `.env` are included; in-tree `.gitignore` is
    // still honoured, but the user's global/ancestor gitignores are not, so scans are
    // reproducible regardless of where the project lives.
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(false)
        .git_global(false)
        .build();
    for dent in walker.flatten() {
        let path = dent.path();
        if path
            .components()
            .any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
        {
            continue;
        }
        if dent.file_type().map(|t| t.is_file()).unwrap_or(false) {
            if let Ok(md) = dent.metadata() {
                if md.len() <= MAX_FILE_SIZE {
                    out.push(path.to_path_buf());
                }
            }
        }
    }
    out
}
