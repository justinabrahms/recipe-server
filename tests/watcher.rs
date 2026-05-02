//! Smoke test for the file watcher: write a new recipe and ensure the index updates.

use recipes::index::watch::{spawn_watcher, SharedIndex};
use recipes::index::{build_index, Index};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[tokio::test(flavor = "multi_thread")]
async fn watcher_picks_up_new_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("alpha.cook"),
        ">> title: Alpha\n\nMix @flour{1%g}.\n",
    )
    .unwrap();

    let initial = build_index(dir.path()).unwrap();
    let shared: SharedIndex = Arc::new(RwLock::new(initial));
    spawn_watcher(dir.path().to_path_buf(), shared.clone())
        .await
        .unwrap();
    assert_eq!(shared.read().await.recipe_count(), 1);

    std::fs::write(
        dir.path().join("beta.cook"),
        ">> title: Beta\n\nMix @flour{2%g}.\n",
    )
    .unwrap();

    // Wait up to 4s for the debounced rebuild (500ms debounce).
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if shared.read().await.recipe_count() >= 2 {
            return;
        }
        if Instant::now() >= deadline {
            let count = shared.read().await.recipe_count();
            panic!("watcher never picked up new file (count={count})");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn watcher_picks_up_modification() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("alpha-v1.cook"),
        ">> title: Alpha\n\nMix @flour{1%g}.\n",
    )
    .unwrap();
    let initial = build_index(dir.path()).unwrap();
    let shared: SharedIndex = Arc::new(RwLock::new(initial));
    spawn_watcher(dir.path().to_path_buf(), shared.clone())
        .await
        .unwrap();
    assert_eq!(
        shared.read().await.families.values().next().unwrap().title,
        "Alpha"
    );

    std::fs::write(
        dir.path().join("alpha-v1.cook"),
        ">> title: Alpha Renamed\n\nMix @flour{1%g}.\n",
    )
    .unwrap();

    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let title = shared
            .read()
            .await
            .families
            .values()
            .next()
            .unwrap()
            .title
            .clone();
        if title == "Alpha Renamed" {
            return;
        }
        if Instant::now() >= deadline {
            panic!("watcher never picked up modification (title={title})");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    #[allow(unreachable_code)]
    {
        let _ = Index::default();
    }
}
