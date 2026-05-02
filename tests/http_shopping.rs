//! HTTP integration tests for the shopping flow.

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

async fn make_app() -> axum::Router {
    let idx = build_index(&fixtures()).unwrap();
    let shared: SharedIndex = Arc::new(RwLock::new(idx));
    routes::router(shared, "/", None)
}

#[tokio::test]
async fn post_shopping_redirects_then_renders() {
    let app = make_app().await;

    let body = "slugs%5B%5D=carbonara&slugs%5B%5D=garlic-bread";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shopping")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.starts_with("/shopping/"), "got {location}");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&location)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.contains("Shopping list"));
    assert!(html.contains("baguette"));
    assert!(html.contains("Apple Notes"));
}

#[tokio::test]
async fn shopping_text_format_is_plain() {
    let app = make_app().await;

    let body = "slugs%5B%5D=garlic-bread";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shopping")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("{location}?format=text"))
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
    assert!(ct.starts_with("text/plain"));
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines().filter(|l| !l.is_empty()) {
        assert!(
            !line.starts_with('-'),
            "Notes-friendly format must not lead with `-`: {line:?}"
        );
    }
    assert!(text.contains("baguette"));
}

#[tokio::test]
async fn html_view_links_source_recipes() {
    let app = make_app().await;
    let body = "slugs%5B%5D=carbonara";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shopping")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(&location)
                .header("host", "recipes.example.com")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let html = String::from_utf8_lossy(&bytes);
    assert!(
        html.contains(
            r#"<a href="https://recipes.example.com/r/carbonara">Spaghetti Carbonara</a>"#
        ),
        "source link missing: {html}"
    );
}

#[tokio::test]
async fn public_url_overrides_request_headers() {
    let idx = build_index(&fixtures()).unwrap();
    let shared: SharedIndex = Arc::new(RwLock::new(idx));
    let app = routes::router(shared, "/", Some("https://recipes.example.com".to_string()));

    let body = "slugs%5B%5D=carbonara";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shopping")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Even with deliberately misleading Host headers, --public-url wins.
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("{location}?format=text"))
                .header("host", "internal.local")
                .header("x-forwarded-proto", "http")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("https://recipes.example.com/r/carbonara"),
        "public_url did not override headers: {text}"
    );
    assert!(!text.contains("internal.local"));
}

#[tokio::test]
async fn multiplier_via_form_repeats_batches() {
    let app = make_app().await;

    // ×3 batches of carbonara (declared 2 servings → 6 effective servings)
    let body = "slugs%5B%5D=carbonara&multiplier%5Bcarbonara%5D=3";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shopping")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("{location}?format=text"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("600 g spaghetti"), "got: {text}");
}

#[tokio::test]
async fn unknown_token_404() {
    let app = make_app().await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/shopping/notarealtoken")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn servings_override_via_form() {
    let app = make_app().await;

    let body = "slugs%5B%5D=carbonara&servings%5Bcarbonara%5D=4";
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shopping")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let location = resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("{location}?format=text"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("400 g spaghetti"), "got: {text}");
}
