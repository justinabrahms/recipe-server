//! Coverage for non-fatal index errors.

use recipes::index::build_index;

fn write(p: std::path::PathBuf, content: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

#[test]
fn slug_collision_does_not_fail_build_but_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("a/My_Dish-v1.cook"),
        ">> title: A\n\nMix @flour{1%g}.\n",
    );
    write(
        dir.path().join("b/MY-DISH-v1.cook"),
        ">> title: B\n\nMix @sugar{1%g}.\n",
    );
    let idx = build_index(dir.path()).unwrap();
    assert_eq!(
        idx.families.len(),
        1,
        "one family wins, the other is an error"
    );
    assert!(
        idx.errors
            .iter()
            .any(|e| e.message.contains("slug collision")),
        "errors: {:?}",
        idx.errors
    );
}

#[test]
fn ignores_sync_conflict_files_with_warning() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("foo-v1.cook"),
        ">> title: Foo\n\nMix @flour{1%g}.\n",
    );
    write(
        dir.path().join("foo-v1 (conflicted copy 2024-04-01).cook"),
        ">> title: Foo\n\nMix @flour{1%g}.\n",
    );
    let idx = build_index(dir.path()).unwrap();
    assert_eq!(idx.recipe_count(), 1);
    assert!(
        idx.warnings
            .iter()
            .any(|w| w.message.contains("sync-conflict")),
        "warnings: {:?}",
        idx.warnings
    );
}

#[test]
fn parse_error_in_one_file_does_not_break_index() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("good.cook"),
        ">> title: Good\n\nMix @flour{1%g}.\n",
    );
    write(
        dir.path().join("bad.cook"),
        ">> title: Bad\n\nMix @flour ~broken.\n",
    );
    let idx = build_index(dir.path()).unwrap();
    // `good` should still be present.
    assert!(idx.families.keys().any(|k| k.as_str() == "good"));
}

#[test]
fn populates_mtime() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path().join("foo-v1.cook"),
        ">> title: Foo\n\nMix @flour{1%g}.\n",
    );
    let idx = build_index(dir.path()).unwrap();
    let fam = idx.families.values().next().unwrap();
    assert!(fam.current().mtime.is_some());
}
