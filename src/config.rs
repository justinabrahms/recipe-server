use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn require_recipes_dir(opt: Option<PathBuf>) -> Result<PathBuf> {
    let p = opt.context("--recipes-dir or RECIPES_DIR must be set")?;
    let abs = std::fs::canonicalize(&p)
        .with_context(|| format!("recipes dir not accessible: {}", p.display()))?;
    if !abs.is_dir() {
        anyhow::bail!("recipes dir is not a directory: {}", abs.display());
    }
    Ok(abs)
}

pub fn relative_to<'a>(base: &Path, full: &'a Path) -> &'a Path {
    full.strip_prefix(base).unwrap_or(full)
}
