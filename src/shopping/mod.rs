//! Aggregation of selected recipes into a shopping list.
//!
//! Per PLAN.md §7.2: scale ingredients by `requested_servings / declared_servings`,
//! group by `(normalized_name, dimension)`, sum quantities. Mismatched dimensions
//! under the same name are kept as separate line items with a warning glyph.

pub mod render;

use crate::index::scan::VersionKey;
use crate::index::{Index, RecipeFamily};
use crate::recipe::units::{canonicalize, Canonical, Dimension};
use crate::recipe::{declared_servings, recipe_title};
use cooklang::quantity::Value;

/// Threshold below which mass quantities are treated as "a bit of" and
/// merged with any unitless/to-taste entry for the same ingredient.
/// 5 g ≈ a small pinch / one heavy teaspoon — small enough that summing
/// doesn't help anyone.
const TRIVIAL_MASS_GRAMS: f64 = 5.0;

/// One recipe selected for the list, with the version+servings the caller wants.
///
/// `multiplier` and `override_servings` are mutually-exclusive scaling controls.
/// When `override_servings` is set, the recipe is scaled to that absolute
/// number of servings. When `multiplier` is set, the recipe is scaled to
/// `declared * multiplier` servings (e.g. "give me 3 batches of this"). If
/// both are set, `override_servings` wins.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub slug: String,
    pub version: Option<VersionKey>,
    pub override_servings: Option<u32>,
    pub multiplier: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineItem {
    pub display_name: String,
    pub canonical: Option<Canonical>, // None when only counted-text or unscalable
    pub display: String,
    pub notes: Vec<String>,
    pub sources: Vec<SourceRef>,
    pub warning: Option<String>,
    /// Aisle/store-section the ingredient belongs to per `aisle.conf`. `None`
    /// if no aisle file is configured or the ingredient isn't listed.
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceRef {
    pub slug: String,
    pub title: String,
    pub version: VersionKey,
    pub servings: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolutionError {
    pub slug: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ShoppingList {
    pub items: Vec<LineItem>,
    pub sources: Vec<SourceRef>,
    pub errors: Vec<ResolutionError>,
}

/// Aggregate selected recipes against the index.
pub fn aggregate(idx: &Index, selections: &[Selection]) -> ShoppingList {
    let mut bins: Vec<Bin> = Vec::new();
    let mut sources: Vec<SourceRef> = Vec::new();
    let mut errors: Vec<ResolutionError> = Vec::new();

    for sel in selections {
        let slug = crate::index::Slug::from_base(&sel.slug);
        let Some(family) = idx.families.get(&slug) else {
            errors.push(ResolutionError {
                slug: sel.slug.clone(),
                message: "no such recipe".into(),
            });
            continue;
        };
        let version = match sel.version {
            Some(k) => match family.versions.iter().find(|v| v.key == k) {
                Some(v) => v,
                None => {
                    errors.push(ResolutionError {
                        slug: sel.slug.clone(),
                        message: format!("version {k} not found"),
                    });
                    continue;
                }
            },
            None => family.current(),
        };
        let recipe = &version.parsed.recipe;
        let declared = declared_servings(recipe).max(1);
        let requested = match (sel.override_servings, sel.multiplier) {
            (Some(s), _) => s.max(1),
            (None, Some(m)) => declared.saturating_mul(m.max(1)).max(1),
            (None, None) => declared,
        };
        let factor = requested as f64 / declared as f64;
        let title = recipe_title(recipe).unwrap_or_else(|| family.base.clone());

        let source = SourceRef {
            slug: family.slug.to_string(),
            title: title.clone(),
            version: version.key,
            servings: requested,
        };
        sources.push(source.clone());

        for ing in &recipe.ingredients {
            // Skip references — the cooklang parser duplicates the underlying ingredient
            // when the recipe references itself or another. We only care about definitions.
            if !ing.relation.is_definition() {
                continue;
            }
            let name = ing.name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let display_name = name.clone();
            let normalized = name.to_ascii_lowercase();

            let qty = ing.quantity.as_ref();
            let (canonical_opt, fallback_value) = match qty {
                Some(q) => match q.value() {
                    Value::Number(n) => {
                        let scaled = n.value() * factor;
                        match canonicalize(scaled, q.unit()) {
                            Ok(c) => (Some(c), None),
                            Err(e) => {
                                errors.push(ResolutionError {
                                    slug: sel.slug.clone(),
                                    message: format!("{}: {}", display_name, e),
                                });
                                continue;
                            }
                        }
                    }
                    Value::Range { start, end } => {
                        // Conservative aggregation: take midpoint.
                        let mid = (start.value() + end.value()) / 2.0;
                        match canonicalize(mid * factor, q.unit()) {
                            Ok(c) => (Some(c), None),
                            Err(e) => {
                                errors.push(ResolutionError {
                                    slug: sel.slug.clone(),
                                    message: format!("{}: {}", display_name, e),
                                });
                                continue;
                            }
                        }
                    }
                    Value::Text(s) => (None, Some(s.clone())),
                },
                None => (None, None),
            };

            // Find or create a bin keyed by (normalized_name, dimension).
            // None+None merges (e.g. "@salt{}" repeated in multiple recipes).
            let bin_key = canonical_opt.map(|c| c.dimension);
            let pos = bins
                .iter()
                .position(|b| b.normalized == normalized && b.canonical_dim == bin_key);

            let pos = match pos {
                Some(i) => i,
                None => {
                    bins.push(Bin {
                        normalized: normalized.clone(),
                        display_name: display_name.clone(),
                        canonical_dim: bin_key,
                        amount: 0.0,
                        text_value: fallback_value.clone(),
                        notes: Vec::new(),
                        sources: Vec::new(),
                        had_trivial_amount: false,
                    });
                    bins.len() - 1
                }
            };

            if let Some(c) = canonical_opt {
                bins[pos].amount += c.amount;
            }
            if let Some(note) = &ing.note {
                let n = note.trim().to_string();
                if !n.is_empty() && !bins[pos].notes.contains(&n) {
                    bins[pos].notes.push(n);
                }
            }
            if !bins[pos]
                .sources
                .iter()
                .any(|s: &SourceRef| s.slug == source.slug && s.version == source.version)
            {
                bins[pos].sources.push(source.clone());
            }
        }
    }

    // Coalesce trivially-small mass bins: anything < TRIVIAL_MASS_GRAMS
    // becomes "a bit" and merges with same-name unitless bin if one exists.
    let mut i = 0;
    while i < bins.len() {
        let demote = matches!(bins[i].canonical_dim, Some(Dimension::Mass))
            && bins[i].amount < TRIVIAL_MASS_GRAMS;
        if demote {
            // Look for an existing None-dim bin with the same name to merge into.
            let target = bins
                .iter()
                .position(|b| b.normalized == bins[i].normalized && b.canonical_dim.is_none());
            if let Some(j) = target {
                if j != i {
                    let removed = bins.remove(i);
                    let target_idx = if j > i { j - 1 } else { j };
                    let target = &mut bins[target_idx];
                    target.had_trivial_amount = true;
                    for n in removed.notes {
                        if !target.notes.contains(&n) {
                            target.notes.push(n);
                        }
                    }
                    for s in removed.sources {
                        if !target
                            .sources
                            .iter()
                            .any(|x| x.slug == s.slug && x.version == s.version)
                        {
                            target.sources.push(s);
                        }
                    }
                    continue;
                }
            }
            // No sibling — convert this bin in place.
            bins[i].canonical_dim = None;
            bins[i].amount = 0.0;
            bins[i].had_trivial_amount = true;
        }
        i += 1;
    }

    // Cross-bin warning: same normalized name appears under multiple dimensions.
    let mut name_dims: std::collections::HashMap<String, Vec<Dimension>> =
        std::collections::HashMap::new();
    for b in &bins {
        if let Some(d) = b.canonical_dim {
            name_dims.entry(b.normalized.clone()).or_default().push(d);
        }
    }
    let conflicting: std::collections::HashSet<String> = name_dims
        .into_iter()
        .filter(|(_, dims)| dims.iter().collect::<std::collections::HashSet<_>>().len() > 1)
        .map(|(n, _)| n)
        .collect();

    // Look up aisle/category info from the index's aisle.conf, if present.
    // We parse on each call (cheap) because `AisleConf<'_>` borrows from the
    // source string and can't be cached across calls without lifetime gymnastics.
    let aisle_parsed =
        idx.aisle_source.as_ref().and_then(|s| {
            match cooklang::aisle::parse_lenient(s).into_result() {
                Ok((conf, _)) => Some(conf),
                Err(_) => None,
            }
        });
    let aisle_index = aisle_parsed.as_ref().map(|c| c.ingredients_info());

    let mut items: Vec<LineItem> = bins
        .into_iter()
        .map(|b| {
            let warning = if conflicting.contains(&b.normalized) {
                Some(format!(
                    "mixed units across recipes ({})",
                    b.canonical_dim
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "n/a".into())
                ))
            } else {
                None
            };
            let display = match b.canonical_dim {
                Some(d) => crate::recipe::units::format_display(Canonical {
                    amount: b.amount,
                    dimension: d,
                }),
                None => match (&b.text_value, b.had_trivial_amount) {
                    (Some(t), _) => t.clone(),
                    (None, true) => "a bit of".to_string(),
                    (None, false) => "to taste".to_string(),
                },
            };
            let category = aisle_index
                .as_ref()
                .and_then(|m| m.get(&b.normalized))
                .map(|info| info.category.to_string());
            LineItem {
                display_name: b.display_name,
                canonical: b.canonical_dim.map(|d| Canonical {
                    amount: b.amount,
                    dimension: d,
                }),
                display,
                notes: b.notes,
                sources: b.sources,
                warning,
                category,
            }
        })
        .collect();

    // Sort:
    //   - if aisle.conf is present: by aisle order (categories listed top-to-bottom
    //     follow the file), uncategorised items go to the end as "Other".
    //   - otherwise: dimensioned items first (alpha), then unitless.
    if let Some(conf) = aisle_parsed.as_ref() {
        items.sort_by(|a, b| {
            let ka = aisle_sort_key(conf, a);
            let kb = aisle_sort_key(conf, b);
            ka.cmp(&kb)
        });
    } else {
        items.sort_by(|a, b| {
            let ka = (a.canonical.is_none(), a.display_name.to_lowercase());
            let kb = (b.canonical.is_none(), b.display_name.to_lowercase());
            ka.cmp(&kb)
        });
    }

    ShoppingList {
        items,
        sources,
        errors,
    }
}

/// Sort key for an aisle-grouped list. Uncategorised items sort last under
/// `usize::MAX`; within a category we keep the file's primary-name order
/// (`ingredient_sort_key`'s second tuple element), tie-breaking by display name.
fn aisle_sort_key(
    conf: &cooklang::aisle::AisleConf<'_>,
    item: &LineItem,
) -> (usize, usize, String) {
    let lower = item.display_name.to_ascii_lowercase();
    let (cat_idx, igr_idx) = conf
        .ingredient_sort_key(&lower)
        .unwrap_or((usize::MAX, usize::MAX));
    (cat_idx, igr_idx, lower)
}

/// Resolve `Selection` from the form-encoded shape `slug` or `slug@v1` or `slug@v1-2`.
pub fn parse_selection(s: &str) -> Selection {
    if let Some((slug, v)) = s.split_once('@') {
        Selection {
            slug: slug.to_string(),
            version: VersionKey::parse(v),
            ..Default::default()
        }
    } else {
        Selection {
            slug: s.to_string(),
            ..Default::default()
        }
    }
}

/// Pretty-print a Selection for the URL footer.
pub fn selection_label(family: &RecipeFamily, sel: &Selection) -> String {
    let v = sel.version.or_else(|| Some(family.current().key)).unwrap();
    if let Some(s) = sel.override_servings {
        format!("{}@{} (×{} servings)", family.slug, v, s)
    } else {
        format!("{}@{}", family.slug, v)
    }
}

struct Bin {
    normalized: String,
    display_name: String,
    canonical_dim: Option<Dimension>,
    amount: f64,
    text_value: Option<String>,
    notes: Vec<String>,
    sources: Vec<SourceRef>,
    /// Set when one or more contributing entries were demoted from
    /// `< TRIVIAL_MASS_GRAMS` mass values. Influences the display label.
    had_trivial_amount: bool,
}
