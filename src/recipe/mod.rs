pub mod parse;
pub mod units;

pub use parse::{parse_file, parse_str, ParseError, ParseOutcome};

use cooklang::Recipe;

/// Pull the recipe title from `>> title:` metadata.
pub fn recipe_title(recipe: &Recipe) -> Option<String> {
    recipe.metadata.title().map(|s| s.to_string())
}

/// `>> changelog:` metadata, if present.
pub fn recipe_changelog(recipe: &Recipe) -> Option<String> {
    recipe
        .metadata
        .map
        .get("changelog")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Declared servings, falling back to `1` if missing or non-numeric.
pub fn declared_servings(recipe: &Recipe) -> u32 {
    recipe
        .metadata
        .servings()
        .and_then(|s| s.as_number())
        .unwrap_or(1)
}
