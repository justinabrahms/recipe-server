use crate::cli::ServeArgs;
use crate::config;
use crate::index::watch::SharedIndex;
use crate::index::{build_index, watch};
use crate::server;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

pub fn run(recipes_dir: Option<PathBuf>, args: ServeArgs) -> Result<()> {
    let root = config::require_recipes_dir(recipes_dir)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        let initial = build_index(&root)?;
        info!(
            family_count = initial.family_count(),
            recipe_count = initial.recipe_count(),
            errors = initial.errors.len(),
            "initial index built"
        );
        let shared: SharedIndex = Arc::new(RwLock::new(initial));

        watch::spawn_watcher(root.clone(), shared.clone()).await?;
        server::run(args, shared).await
    })
}
