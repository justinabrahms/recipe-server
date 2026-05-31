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

#[test]
fn groups_meal_categories_in_display_order() {
    let dir = tempfile::tempdir().unwrap();
    let recipes = [
        ("dinners/stew.cook", ">> title: Stew\n\nCook @beans{}.\n"),
        ("breakfast/oats.cook", ">> title: Oats\n\nCook @oats{}.\n"),
        ("snacks/popcorn.cook", ">> title: Popcorn\n\nPop @corn{}.\n"),
        ("lunch/salad.cook", ">> title: Salad\n\nToss @lettuce{}.\n"),
        ("misc/water.cook", ">> title: Water\n\nPour @water{}.\n"),
    ];
    for (path, body) in recipes {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    let idx = build_index(dir.path()).unwrap();
    let categories: Vec<_> = idx
        .families_by_category()
        .into_iter()
        .map(|(category, _)| category)
        .collect();

    assert_eq!(
        categories,
        vec!["breakfast", "lunch", "snacks", "dinners", "misc"]
    );
}
