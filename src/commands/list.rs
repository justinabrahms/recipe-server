use crate::config;
use crate::index::build_index;
use anyhow::Result;
use std::path::PathBuf;

pub fn run(recipes_dir: Option<PathBuf>) -> Result<()> {
    let root = config::require_recipes_dir(recipes_dir)?;
    let idx = build_index(&root)?;

    let mut families = idx.families_sorted();
    families.sort_by(|a, b| a.slug.cmp(&b.slug));

    for f in families {
        let cur = f.current();
        println!(
            "{:<32} {:<8} {}",
            f.slug.as_str(),
            cur.key.to_string(),
            f.title
        );
    }
    if !idx.errors.is_empty() {
        eprintln!();
        eprintln!("{} errors:", idx.errors.len());
        for e in &idx.errors {
            eprintln!("  {}: {}", e.path.display(), e.message);
        }
    }
    Ok(())
}
