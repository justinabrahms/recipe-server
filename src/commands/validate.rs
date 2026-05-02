use crate::cli::ValidateArgs;
use crate::index::scan::{parse_recipe_path, FilenameError, VersionKey};
use crate::index::slug::Slug;
use crate::recipe;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// `validate` exits with these codes per the spec:
///   0 — clean
///   1 — invariant violations (parse / collisions / dup versions / malformed)
///   2 — IO / usage
pub fn run(args: ValidateArgs) -> Result<()> {
    let path = match std::fs::canonicalize(&args.path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}: {}", args.path.display(), e);
            std::process::exit(2);
        }
    };

    let mut had_error = false;

    if path.is_file() {
        had_error |= validate_single_file(&path);
    } else if path.is_dir() {
        had_error |= validate_directory(&path);
    } else {
        eprintln!(
            "error: {} is neither a file nor a directory",
            path.display()
        );
        std::process::exit(2);
    }

    if had_error {
        std::process::exit(1);
    }
    Ok(())
}

fn validate_single_file(path: &Path) -> bool {
    if path.extension().and_then(|s| s.to_str()) != Some("cook") {
        eprintln!("error: not a .cook file: {}", path.display());
        return true;
    }
    match recipe::parse_file(path) {
        Ok(out) => {
            for w in &out.warnings {
                eprintln!("WARN  {}: {}", path.display(), w);
            }
            println!("OK    {}", path.display());
            false
        }
        Err(e) => {
            println!("FAIL  {}: {}", path.display(), e);
            true
        }
    }
}

struct PerFile {
    path: PathBuf,
    base: String,
    category: String,
    version: VersionKey,
    slug: Slug,
}

fn validate_directory(root: &Path) -> bool {
    let mut had_error = false;
    let mut files: Vec<PerFile> = Vec::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                eprintln!("FAIL  walk: {e}");
                had_error = true;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !name.ends_with(".cook") {
            continue;
        }
        if name.contains("conflicted copy") || name.contains(".sync-conflict") {
            eprintln!("WARN  {}: ignored sync-conflict file", path.display());
            continue;
        }

        match parse_recipe_path(root, path) {
            Ok(meta) => {
                match recipe::parse_file(path) {
                    Ok(out) => {
                        for w in &out.warnings {
                            eprintln!("WARN  {}: {}", path.display(), w);
                        }
                        // §9 warnings: missing title, missing changelog on non-current
                        if recipe::recipe_title(&out.recipe).is_none() {
                            eprintln!("WARN  {}: missing >> title:", path.display());
                        }
                        println!("OK    {}", path.display());
                    }
                    Err(e) => {
                        println!("FAIL  {}: {}", path.display(), e);
                        had_error = true;
                        continue;
                    }
                }
                files.push(PerFile {
                    path: path.to_path_buf(),
                    base: meta.base,
                    category: meta.category,
                    version: meta.version,
                    slug: meta.slug,
                });
            }
            Err(e @ FilenameError::MalformedVersion(_) | e @ FilenameError::EmptyBase(_)) => {
                println!("FAIL  {}: {}", path.display(), e);
                had_error = true;
            }
            Err(e) => {
                println!("FAIL  {}: {}", path.display(), e);
                had_error = true;
            }
        }
    }

    // Duplicate (base, category, version)
    let mut by_key: HashMap<(String, String, VersionKey), &PerFile> = HashMap::new();
    for f in &files {
        let key = (f.base.clone(), f.category.clone(), f.version);
        if let Some(prev) = by_key.insert(key, f) {
            println!(
                "FAIL  {}: duplicate version {} (also {})",
                f.path.display(),
                f.version,
                prev.path.display()
            );
            had_error = true;
        }
    }

    // Slug collisions across distinct (base, category) pairs.
    let mut by_slug: HashMap<Slug, (String, String, PathBuf)> = HashMap::new();
    for f in &files {
        match by_slug.entry(f.slug.clone()) {
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert((f.base.clone(), f.category.clone(), f.path.clone()));
            }
            std::collections::hash_map::Entry::Occupied(o) => {
                let (prev_base, prev_cat, prev_path) = o.get();
                if prev_base != &f.base || prev_cat != &f.category {
                    println!(
                        "FAIL  {}: slug `{}` also produced by {}",
                        f.path.display(),
                        f.slug,
                        prev_path.display()
                    );
                    had_error = true;
                }
            }
        }
    }

    had_error
}
