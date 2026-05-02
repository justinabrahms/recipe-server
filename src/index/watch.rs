//! M2 file watcher. Spawns a notify-based debounced watcher that rebuilds the
//! whole Index on any change. Rebuild is cheap up to a few thousand recipes;
//! we choose simplicity over surgical updates.

use super::{build_index, Index};
use anyhow::Result;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub type SharedIndex = Arc<RwLock<Index>>;

pub async fn spawn_watcher(root: PathBuf, shared: SharedIndex) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(8);

    // notify needs a sync send-able channel; bridge via std mpsc.
    let (raw_tx, raw_rx) = std::sync::mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), None, raw_tx)?;
    debouncer.watch(&root, RecursiveMode::Recursive)?;

    let watch_root = root.clone();
    std::thread::spawn(move || {
        // Keep debouncer alive for the lifetime of this thread.
        let _keep = debouncer;
        while let Ok(events) = raw_rx.recv() {
            match events {
                Ok(events) if !events.is_empty() => {
                    if events.iter().any(|e| relevant_event(&e.event)) {
                        let _ = tx.blocking_send(());
                    }
                }
                Ok(_) => {}
                Err(errs) => {
                    for e in errs {
                        warn!(?e, root = %watch_root.display(), "watcher error");
                    }
                }
            }
        }
    });

    let rebuild_root = root.clone();
    let rebuild_shared = shared.clone();
    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            // Drain any pending burst before rebuilding.
            while rx.try_recv().is_ok() {}
            let root = rebuild_root.clone();
            let new_idx = tokio::task::spawn_blocking(move || build_index(&root)).await;
            match new_idx {
                Ok(Ok(idx)) => {
                    info!(
                        family_count = idx.family_count(),
                        recipe_count = idx.recipe_count(),
                        errors = idx.errors.len(),
                        "index rebuilt"
                    );
                    let mut w = rebuild_shared.write().await;
                    *w = idx;
                }
                Ok(Err(e)) => error!(?e, "rebuild failed"),
                Err(e) => error!(?e, "rebuild task panicked"),
            }
        }
    });

    Ok(())
}

fn relevant_event(ev: &notify::Event) -> bool {
    use notify::EventKind;
    if !matches!(
        ev.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }
    ev.paths.iter().any(|p| {
        let ext = p.extension().and_then(|s| s.to_str());
        let name = p.file_name().and_then(|s| s.to_str());
        ext == Some("cook") || name == Some("aisle.conf")
    })
}
