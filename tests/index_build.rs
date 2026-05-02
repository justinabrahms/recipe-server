use recipes::index::build_index;
use recipes::index::scan::VersionKey;
use recipes::index::slug::Slug;

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn discovers_carbonara_versions() {
    let idx = build_index(&fixtures()).unwrap();
    let fam = idx.families.get(&Slug::from_base("carbonara")).unwrap();
    assert_eq!(fam.title, "Spaghetti Carbonara");
    assert_eq!(fam.category, "mains/pasta");
    let keys: Vec<VersionKey> = fam.versions.iter().map(|v| v.key).collect();
    assert_eq!(
        keys,
        vec![
            VersionKey { major: 1, minor: 0 },
            VersionKey { major: 1, minor: 1 },
            VersionKey { major: 2, minor: 0 },
        ]
    );
    assert_eq!(fam.current().key, VersionKey { major: 2, minor: 0 });
    assert!(idx.errors.is_empty(), "errors: {:?}", idx.errors);
}

#[test]
fn discovers_unversioned_recipe_as_v1() {
    let idx = build_index(&fixtures()).unwrap();
    let fam = idx.families.get(&Slug::from_base("garlic-bread")).unwrap();
    assert_eq!(fam.title, "Garlic Bread");
    assert_eq!(fam.versions.len(), 1);
    assert_eq!(fam.current().key, VersionKey::V1);
}
