use crate::cli::ShoppingArgs;
use crate::config;
use crate::index::build_index;
use crate::shopping::render::{Format, RenderOpts};
use crate::shopping::{aggregate, parse_selection, Selection};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub fn run(recipes_dir: Option<PathBuf>, args: ShoppingArgs) -> Result<()> {
    let root = config::require_recipes_dir(recipes_dir)?;
    let idx = build_index(&root)?;

    let overrides = parse_servings_overrides(args.servings.as_deref())?;

    let mut selections: Vec<Selection> = args.recipes.iter().map(|s| parse_selection(s)).collect();
    for sel in &mut selections {
        if let Some(v) = overrides.get(&sel.slug) {
            sel.override_servings = Some(*v);
        }
    }

    let format = Format::parse(&args.format)
        .ok_or_else(|| anyhow::anyhow!("unknown format `{}`", args.format))?;
    let list = aggregate(&idx, &selections);

    if !list.errors.is_empty() {
        for e in &list.errors {
            eprintln!("warn: {}: {}", e.slug, e.message);
        }
    }
    let opts = RenderOpts {
        recipe_link_base: args.link_base.as_deref(),
    };
    print!(
        "{}",
        crate::shopping::render::render_with(&list, format, &opts)
    );
    Ok(())
}

fn parse_servings_overrides(s: Option<&str>) -> Result<HashMap<String, u32>> {
    let mut out = HashMap::new();
    let Some(s) = s else { return Ok(out) };
    for pair in s.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (slug, count) = pair
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--servings expects slug=N pairs, got `{pair}`"))?;
        let count: u32 = count
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid servings count in `{pair}`"))?;
        if count == 0 {
            anyhow::bail!("servings must be > 0 in `{pair}`");
        }
        out.insert(slug.trim().to_string(), count);
    }
    Ok(out)
}
