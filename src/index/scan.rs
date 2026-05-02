use super::slug::Slug;
use std::fmt;
use std::path::{Path, PathBuf};

/// (MAJOR, MINOR). Implicit minor is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VersionKey {
    pub major: u32,
    pub minor: u32,
}

impl VersionKey {
    pub const V1: VersionKey = VersionKey { major: 1, minor: 0 };

    pub fn parse(s: &str) -> Option<Self> {
        // accepts: "v1", "v1-2"
        let rest = s.strip_prefix('v')?;
        let mut parts = rest.split('-');
        let major: u32 = parts.next()?.parse().ok()?;
        if major == 0 {
            return None;
        }
        let minor: u32 = match parts.next() {
            Some(m) => {
                let v = m.parse().ok()?;
                if v == 0 {
                    return None;
                }
                v
            }
            None => 0,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(VersionKey { major, minor })
    }
}

impl fmt::Display for VersionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minor == 0 {
            write!(f, "v{}", self.major)
        } else {
            write!(f, "v{}-{}", self.major, self.minor)
        }
    }
}

/// Parsed information about a recipe file's path.
#[derive(Debug, Clone)]
pub struct RecipeFileMeta {
    pub path: PathBuf,
    pub category: String, // relative directory portion, slash-joined; "" for root
    pub base: String,     // base name (pre-slug)
    pub slug: Slug,
    pub version: VersionKey,
    pub explicit_version: bool,
}

/// Parse a `.cook` file path under `root`.
///
/// Returns `Err` if the filename doesn't match the version-suffix grammar.
pub fn parse_recipe_path(root: &Path, full: &Path) -> Result<RecipeFileMeta, FilenameError> {
    let rel = full.strip_prefix(root).unwrap_or(full);

    // Reject sync-conflict filenames at scan-time so they never become recipes.
    let name = rel
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| FilenameError::NonUtf8(full.to_path_buf()))?;

    let stem = name
        .strip_suffix(".cook")
        .ok_or_else(|| FilenameError::NotCookFile(full.to_path_buf()))?;

    let (base, version, explicit) = split_version_suffix(stem)?;

    let category = rel
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default();

    let slug = Slug::from_base(base);

    Ok(RecipeFileMeta {
        path: full.to_path_buf(),
        category,
        base: base.to_string(),
        slug,
        version,
        explicit_version: explicit,
    })
}

/// Split a stem into `(base, version, has_explicit_version)`.
///
/// `food-v1.cook` → ("food", v1, true)
/// `food-v1-2.cook` → ("food", v1-2, true)
/// `food.cook` → ("food", v1, false)
fn split_version_suffix(stem: &str) -> Result<(&str, VersionKey, bool), FilenameError> {
    // Look for last "-v" segment that is followed by a valid version.
    // Try the two-segment form first ("-v<N>-<M>"), then the single form.
    if let Some(pos) = stem.rfind("-v") {
        // The version candidate is everything after "-v".
        let candidate_after_v = &stem[pos + 1..]; // includes the leading 'v'
        if let Some(version) = VersionKey::parse(candidate_after_v) {
            let base = &stem[..pos];
            if base.is_empty() {
                return Err(FilenameError::EmptyBase(stem.to_string()));
            }
            return Ok((base, version, true));
        }
        // Could be malformed: starts like a version suffix but doesn't parse.
        // Distinguish "-vfoo" (looks like a version attempt) from "-vegan" (not a version).
        // Heuristic: if the chars after "-v" start with a digit, treat as malformed.
        let after = &stem[pos + 2..];
        if after
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return Err(FilenameError::MalformedVersion(stem.to_string()));
        }
    }
    if stem.is_empty() {
        return Err(FilenameError::EmptyBase(stem.to_string()));
    }
    Ok((stem, VersionKey::V1, false))
}

#[derive(Debug, thiserror::Error)]
pub enum FilenameError {
    #[error("non-UTF8 filename: {}", .0.display())]
    NonUtf8(PathBuf),
    #[error("not a .cook file: {}", .0.display())]
    NotCookFile(PathBuf),
    #[error("malformed version suffix in {0:?}; expected -v<N> or -v<N>-<M>")]
    MalformedVersion(String),
    #[error("empty recipe base name in {0:?}")]
    EmptyBase(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(name: &str) -> RecipeFileMeta {
        let root = Path::new("/r");
        parse_recipe_path(root, &root.join(name)).unwrap()
    }

    #[test]
    fn no_version_is_v1() {
        let m = parse("carbonara.cook");
        assert_eq!(m.version, VersionKey::V1);
        assert!(!m.explicit_version);
        assert_eq!(m.slug.as_str(), "carbonara");
    }

    #[test]
    fn major_only() {
        let m = parse("carbonara-v2.cook");
        assert_eq!(m.version, VersionKey { major: 2, minor: 0 });
        assert!(m.explicit_version);
    }

    #[test]
    fn major_minor() {
        let m = parse("carbonara-v1-2.cook");
        assert_eq!(m.version, VersionKey { major: 1, minor: 2 });
    }

    #[test]
    fn category_from_subdir() {
        let root = Path::new("/r");
        let m = parse_recipe_path(root, &root.join("mains/pasta/carbonara-v1.cook")).unwrap();
        assert_eq!(m.category, "mains/pasta");
        assert_eq!(m.base, "carbonara");
    }

    #[test]
    fn vegan_is_not_a_version() {
        // "-vegan" must not be parsed as a version
        let m = parse("vegan-stew.cook");
        assert_eq!(m.base, "vegan-stew");
        assert_eq!(m.version, VersionKey::V1);
    }

    #[test]
    fn malformed_v0() {
        let root = Path::new("/r");
        let r = parse_recipe_path(root, &root.join("foo-v0.cook"));
        assert!(matches!(r, Err(FilenameError::MalformedVersion(_))));
    }

    #[test]
    fn malformed_extra_dash() {
        let root = Path::new("/r");
        let r = parse_recipe_path(root, &root.join("foo-v1-2-3.cook"));
        assert!(matches!(r, Err(FilenameError::MalformedVersion(_))));
    }

    #[test]
    fn slug_lowercased() {
        let m = parse("Spaghetti_Carbonara-v1.cook");
        assert_eq!(m.slug.as_str(), "spaghetti-carbonara");
        assert_eq!(m.base, "Spaghetti_Carbonara");
    }

    #[test]
    fn version_key_ordering() {
        let a = VersionKey { major: 1, minor: 0 };
        let b = VersionKey { major: 1, minor: 1 };
        let c = VersionKey { major: 2, minor: 0 };
        assert!(a < b);
        assert!(b < c);
    }
}
