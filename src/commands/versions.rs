use crate::cli::VersionsArgs;
use crate::config;
use crate::index::{build_index, Slug};
use crate::recipe;
use anyhow::Result;
use std::path::PathBuf;

pub fn run(recipes_dir: Option<PathBuf>, args: VersionsArgs) -> Result<()> {
    let root = config::require_recipes_dir(recipes_dir)?;
    let idx = build_index(&root)?;
    let slug = Slug::from_base(&args.slug);
    let family = idx
        .families
        .get(&slug)
        .ok_or_else(|| anyhow::anyhow!("no recipe family with slug `{}`", slug))?;

    println!("{}  ({})", family.title, family.slug);
    println!();
    println!("{:<8}  {:<25}  changelog", "version", "file");
    for v in &family.versions {
        let cl = recipe::recipe_changelog(&v.parsed.recipe).unwrap_or_default();
        let path = v
            .path
            .strip_prefix(&root)
            .unwrap_or(&v.path)
            .display()
            .to_string();
        println!("{:<8}  {:<25}  {}", v.key.to_string(), path, cl);
    }
    Ok(())
}
