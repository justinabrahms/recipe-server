//! Drive the axum router directly with tower::ServiceExt — no binding required.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use recipes::index::{build_index, watch::SharedIndex};
use recipes::server::routes;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn fixtures() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

async fn body_text(resp: axum::response::Response) -> (StatusCode, String) {
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn make_app() -> axum::Router {
    let idx = build_index(&fixtures()).unwrap();
    let shared: SharedIndex = Arc::new(RwLock::new(idx));
    routes::router(shared, "/", None)
}

#[tokio::test]
async fn home_lists_all_families() {
    let app = make_app().await;
    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, html) = body_text(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Spaghetti Carbonara"));
    assert!(html.contains("Garlic Bread"));
    assert!(html.contains("/r/carbonara"));
    assert!(html.contains("/r/garlic-bread"));
    assert!(html.contains(r#"name="slugs[]""#));
}

#[tokio::test]
async fn recipe_view_shows_current_version() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/r/carbonara")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, html) = body_text(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Spaghetti Carbonara"));
    assert!(html.contains("v2"));
    assert!(html.contains("guanciale"));
}

#[tokio::test]
async fn recipe_view_specific_version() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/r/carbonara/v/v1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, html) = body_text(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("Initial version"),
        "older version should show changelog"
    );
}

#[tokio::test]
async fn unknown_recipe_is_404() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/r/no-such-recipe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn history_lists_versions_newest_first() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/r/carbonara/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, html) = body_text(resp).await;
    assert_eq!(status, StatusCode::OK);
    let v2 = html.find(">v2<").expect("v2 link present");
    let v1 = html.find(">v1<").expect("v1 link present");
    assert!(v2 < v1, "v2 should appear before v1 (newest first)");
}

#[tokio::test]
async fn health_returns_index_status() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["family_count"], 2);
    assert_eq!(json["recipe_count"], 4);
}

#[tokio::test]
async fn ingredients_panel_deduplicates_repeated_mentions() {
    // Build a tiny tree where salt is mentioned in three steps. The
    // ingredients panel should show salt once, not three times.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dup.cook"),
        ">> title: Dup\n>> servings: 1\n\n\
         Add @salt{1%g} to the pan.\n\n\
         Sprinkle @salt{2%g} on top.\n\n\
         Finish with @salt{} to taste.\n",
    )
    .unwrap();
    let idx = recipes::index::build_index(dir.path()).unwrap();
    let shared: SharedIndex = Arc::new(RwLock::new(idx));
    let app = recipes::server::routes::router(shared, "/", None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/r/dup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, html) = body_text(resp).await;
    assert_eq!(status, StatusCode::OK);

    // Slice out the <ul class="ingredient-list"> block and count salt rows.
    let start = html
        .find(r#"<ul class="ingredient-list""#)
        .expect("ingredient list block");
    let end = html[start..].find("</ul>").unwrap() + start;
    let block = &html[start..end];
    let count = block.matches(r#"class="ingredient">salt</span>"#).count();
    assert_eq!(count, 1, "salt should appear exactly once: {block}");
}

#[tokio::test]
async fn diff_view_renders_unified_diff() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/r/carbonara/diff?from=v1&to=v2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, html) = body_text(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Comparing"), "header missing: {html}");
    assert!(html.contains("class=\"diff\""));
}

#[tokio::test]
async fn static_assets_served() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/static/style.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("text/css"), "content-type was {ct}");
}
