use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_recipes"))
}

#[test]
fn validate_clean_tree_exits_zero() {
    let out = bin().args(["validate", "tests/fixtures"]).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn validate_duplicate_version_fails() {
    let dir = tempfile::tempdir().unwrap();
    // unversioned name + explicit -v1 collide on (base, MAJOR, MINOR)=(foo,1,0)
    std::fs::write(
        dir.path().join("foo.cook"),
        ">> title: A\n\nMix @flour{1%g}.\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("foo-v1.cook"),
        ">> title: A\n\nMix @flour{1%g}.\n",
    )
    .unwrap();

    let out = bin()
        .args(["validate", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("duplicate version"), "stdout: {stdout}");
}

#[test]
fn validate_malformed_version_fails() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("foo-v0.cook"),
        ">> title: A\n\nMix @flour{1%g}.\n",
    )
    .unwrap();

    let out = bin()
        .args(["validate", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("malformed version"), "stdout: {stdout}");
}

#[test]
fn validate_slug_collision_fails() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::write(
        dir.path().join("a/My_Dish-v1.cook"),
        ">> title: One\n\nMix @flour{1%g}.\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b/MY-DISH-v1.cook"),
        ">> title: Two\n\nMix @sugar{1%g}.\n",
    )
    .unwrap();

    let out = bin()
        .args(["validate", dir.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("slug `my-dish`"), "stdout: {stdout}");
}
