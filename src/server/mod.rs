use crate::cli::ServeArgs;
use crate::index::watch::SharedIndex;
use anyhow::Result;

pub mod assets;
pub mod render;
pub mod routes;
pub mod store;

pub async fn run(args: ServeArgs, shared: SharedIndex) -> Result<()> {
    let app = routes::router(shared, &args.base_path, args.public_url.clone());
    // Normalize `/recipes/` → `/recipes` so axum's `nest` matches both.
    let svc = tower::ServiceBuilder::new()
        .layer(tower_http::normalize_path::NormalizePathLayer::trim_trailing_slash())
        .service(app);

    let listener = tokio::net::TcpListener::bind(&args.bind).await?;
    let actual = listener.local_addr()?;
    tracing::info!(
        addr = %actual,
        base_path = %args.base_path,
        public_url = ?args.public_url,
        "listening",
    );
    axum::serve(listener, tower::make::Shared::new(svc)).await?;
    Ok(())
}
