pub mod scan;
pub mod slug;
pub mod watch;

use crate::recipe::{self, ParseOutcome};
use scan::{parse_recipe_path, RecipeFileMeta, VersionKey};
pub use slug::Slug;

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use walkdir::WalkDir;

/// One specific version of a recipe family.
#[derive(Debug, Clone)]
pub struct Version {
    pub key: VersionKey,
    pub path: PathBuf,
    pub explicit: bool,
    pub mtime: Option<SystemTime>,
    pub source: Arc<String>,
    pub parsed: ParseOutcome,
}

/// All versions of one recipe (grouped by base name).
#[derive(Debug, Clone)]
pub struct RecipeFamily {
    pub slug: Slug,
    pub base: String,
    pub category: String,
    pub title: String,
    pub versions: Vec<Version>,
}

impl RecipeFamily {
    pub fn current(&self) -> &Version {
        self.versions
            .last()
            .expect("RecipeFamily always has at least one version")
    }

    pub fn version(&self, key: VersionKey) -> Option<&Version> {
        self.versions.iter().find(|v| v.key == key)
    }
}

/// Errors surfaced via /health and `validate`.
#[derive(Debug, Clone)]
pub struct IndexError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Index {
    pub root: PathBuf,
    pub families: HashMap<Slug, RecipeFamily>,
    pub by_path: HashMap<PathBuf, (Slug, VersionKey)>,
    pub errors: Vec<IndexError>,
    pub warnings: Vec<IndexError>,
    pub last_built: Option<SystemTime>,
    /// Contents of `aisle.conf` at the recipes root, if present. Held as a
    /// string because `cooklang::aisle::AisleConf` borrows from its source.
    /// Callers parse on demand against this slice.
    pub aisle_source: Option<Arc<String>>,
}

impl Index {
    pub fn empty(root: PathBuf) -> Self {
        Index {
            root,
            ..Default::default()
        }
    }

    pub fn family_count(&self) -> usize {
        self.families.len()
    }

    pub fn recipe_count(&self) -> usize {
        self.families.values().map(|f| f.versions.len()).sum()
    }

    /// Sorted list of families for stable UI output.
    pub fn families_sorted(&self) -> Vec<&RecipeFamily> {
        let mut v: Vec<_> = self.families.values().collect();
        v.sort_by_key(|a| a.title.to_lowercase());
        v
    }

    pub fn families_by_category(&self) -> Vec<(String, Vec<&RecipeFamily>)> {
        let mut map: HashMap<String, Vec<&RecipeFamily>> = HashMap::new();
        for f in self.families.values() {
            map.entry(f.category.clone()).or_default().push(f);
        }
        for vs in map.values_mut() {
            vs.sort_by_key(|a| a.title.to_lowercase());
        }
        let mut groups: Vec<_> = map.into_iter().collect();
        groups.sort_by(|(a, _), (b, _)| category_sort_key(a).cmp(&category_sort_key(b)));
        groups
    }
}

fn category_sort_key(category: &str) -> (u8, String) {
    let first = category
        .split('/')
        .next()
        .unwrap_or(category)
        .trim()
        .to_ascii_lowercase();
    let priority = match first.as_str() {
        "breakfast" | "breakfasts" => 0,
        "lunch" | "lunches" => 1,
        "snack" | "snacks" => 2,
        "dinner" | "dinners" | "main" | "mains" => 3,
        _ => 4,
    };
    (priority, category.to_ascii_lowercase())
}

/// Walk `root`, parse every `.cook`, build an Index. Errors are collected, not fatal.
pub fn build_index(root: &Path) -> Result<Index> {
    let mut idx = Index::empty(root.to_path_buf());

    // group: base+category → list of (meta, parsed)
    let mut groups: HashMap<(String, String), Vec<(RecipeFileMeta, FileLoad)>> = HashMap::new();

    for entry in WalkDir::new(root).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                idx.errors.push(IndexError {
                    path: e.path().map(Path::to_path_buf).unwrap_or_default(),
                    message: format!("walk error: {e}"),
                });
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
            idx.warnings.push(IndexError {
                path: path.to_path_buf(),
                message: "ignored sync-conflict file".to_string(),
            });
            continue;
        }

        let meta = match parse_recipe_path(root, path) {
            Ok(m) => m,
            Err(e) => {
                idx.errors.push(IndexError {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
                continue;
            }
        };

        let load = match load_file(path) {
            Ok(l) => l,
            Err(e) => {
                idx.errors.push(IndexError {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                });
                continue;
            }
        };

        groups
            .entry((meta.base.clone(), meta.category.clone()))
            .or_default()
            .push((meta, load));
    }

    // Group same-base across different categories: a base must have one canonical
    // category. If files with the same base appear in multiple categories, treat
    // them as separate families and surface a warning.
    for ((_base, _cat), mut files) in groups {
        files.sort_by_key(|(m, _)| m.version);

        // Duplicate-version detection
        let mut seen: HashMap<VersionKey, &Path> = HashMap::new();
        let mut dup = false;
        for (m, _) in &files {
            if let Some(prev) = seen.insert(m.version, &m.path) {
                idx.errors.push(IndexError {
                    path: m.path.clone(),
                    message: format!(
                        "duplicate version {} (also in {})",
                        m.version,
                        prev.display()
                    ),
                });
                dup = true;
            }
        }
        if dup {
            continue;
        }
        if files.is_empty() {
            continue;
        }

        let head = &files[0].0;
        let slug = head.slug.clone();
        let category = head.category.clone();
        let base = head.base.clone();

        if let Some(existing) = idx.families.get(&slug) {
            // Slug collision: different base or category that maps to the same slug.
            idx.errors.push(IndexError {
                path: head.path.clone(),
                message: format!(
                    "slug collision: {} (already used by {})",
                    slug,
                    existing.versions[0].path.display()
                ),
            });
            continue;
        }

        let mut versions = Vec::with_capacity(files.len());
        for (meta, load) in files {
            let parsed = match recipe::parse_str(&load.source) {
                Ok(o) => o,
                Err(e) => {
                    idx.errors.push(IndexError {
                        path: meta.path.clone(),
                        message: e.to_string(),
                    });
                    continue;
                }
            };
            versions.push(Version {
                key: meta.version,
                path: meta.path.clone(),
                explicit: meta.explicit_version,
                mtime: load.mtime,
                source: Arc::new(load.source),
                parsed,
            });
        }
        if versions.is_empty() {
            continue;
        }

        let title = recipe::recipe_title(&versions.last().unwrap().parsed.recipe)
            .unwrap_or_else(|| base.clone());
        let family = RecipeFamily {
            slug: slug.clone(),
            base,
            category,
            title,
            versions,
        };
        for v in &family.versions {
            idx.by_path.insert(v.path.clone(), (slug.clone(), v.key));
        }
        idx.families.insert(slug, family);
    }

    // Optional aisle.conf at the recipes root.
    let aisle_path = root.join("aisle.conf");
    if aisle_path.is_file() {
        match std::fs::read_to_string(&aisle_path) {
            Ok(s) => {
                // Parse once eagerly to surface errors; the actual lookups
                // re-parse against the borrowed source string.
                match cooklang::aisle::parse_lenient(&s).into_result() {
                    Ok((_conf, report)) => {
                        for w in report.warnings() {
                            idx.warnings.push(IndexError {
                                path: aisle_path.clone(),
                                message: w.message.to_string(),
                            });
                        }
                        idx.aisle_source = Some(Arc::new(s));
                    }
                    Err(report) => {
                        for e in report.errors() {
                            idx.errors.push(IndexError {
                                path: aisle_path.clone(),
                                message: e.message.to_string(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                idx.errors.push(IndexError {
                    path: aisle_path,
                    message: format!("read aisle.conf: {e}"),
                });
            }
        }
    }

    idx.last_built = Some(SystemTime::now());
    Ok(idx)
}

struct FileLoad {
    source: String,
    mtime: Option<SystemTime>,
}

fn load_file(path: &Path) -> std::io::Result<FileLoad> {
    let source = std::fs::read_to_string(path)?;
    let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
    Ok(FileLoad { source, mtime })
}
