use crate::index::scan::VersionKey;
use crate::index::watch::SharedIndex;
use crate::index::Slug;
use crate::server::assets;
use crate::server::render::{self, Layout};
use crate::server::store::ListStore;
use crate::shopping::{self, render::Format, Selection};
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub index: SharedIndex,
    pub base_path: Arc<String>,
    pub lists: Arc<ListStore>,
    /// When set, this overrides any base URL derived from request headers.
    /// Used to embed clickable recipe links in the shopping list output.
    pub public_url: Arc<Option<String>>,
}

pub fn router(index: SharedIndex, base_path: &str, public_url: Option<String>) -> Router {
    let normalized = normalize_base_path(base_path);
    let state = AppState {
        index,
        base_path: Arc::new(normalized.clone()),
        lists: Arc::new(ListStore::new()),
        public_url: Arc::new(public_url.map(|u| u.trim_end_matches('/').to_string())),
    };

    let app = Router::new()
        .route("/", get(home))
        .route("/r/{slug}", get(recipe_latest))
        .route("/r/{slug}/v/{version}", get(recipe_version))
        .route("/r/{slug}/history", get(recipe_history))
        .route("/r/{slug}/diff", get(recipe_diff))
        .route("/shopping", post(shopping_create))
        .route("/shopping/{token}", get(shopping_view))
        .route("/health", get(health))
        .route("/static/{*path}", get(static_asset))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    if normalized == "/" {
        app
    } else {
        Router::new().nest(&normalized, app)
    }
}

fn normalize_base_path(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }
    let with_slash = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    with_slash.trim_end_matches('/').to_string()
}

fn layout<'a>(state: &'a AppState, title: &'a str) -> Layout<'a> {
    Layout {
        base_path: state.base_path.as_str(),
        title,
    }
}

async fn home(State(state): State<AppState>) -> Html<String> {
    let idx = state.index.read().await;
    Html(render::list_page(layout(&state, "All recipes"), &idx))
}

async fn recipe_latest(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    let idx = state.index.read().await;
    let slug = Slug::from_base(&slug);
    let Some(family) = idx.families.get(&slug) else {
        return not_found_response(&state, &format!("No recipe with slug `{slug}`"));
    };
    let last = family.versions.len() - 1;
    Html(render::recipe_view(
        layout(&state, &family.title),
        family,
        last,
    ))
    .into_response()
}

async fn recipe_version(
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> Response {
    let idx = state.index.read().await;
    let slug = Slug::from_base(&slug);
    let Some(family) = idx.families.get(&slug) else {
        return not_found_response(&state, &format!("No recipe with slug `{slug}`"));
    };
    let Some(key) = VersionKey::parse(&version) else {
        return not_found_response(&state, &format!("Bad version `{version}`"));
    };
    let Some(idx_pos) = family.versions.iter().position(|v| v.key == key) else {
        return not_found_response(&state, &format!("Version {version} not found for {slug}"));
    };
    Html(render::recipe_view(
        layout(&state, &family.title),
        family,
        idx_pos,
    ))
    .into_response()
}

async fn recipe_history(State(state): State<AppState>, Path(slug): Path<String>) -> Response {
    let idx = state.index.read().await;
    let slug = Slug::from_base(&slug);
    let Some(family) = idx.families.get(&slug) else {
        return not_found_response(&state, &format!("No recipe with slug `{slug}`"));
    };
    Html(render::history_page(layout(&state, &family.title), family)).into_response()
}

#[derive(serde::Deserialize)]
struct DiffQuery {
    from: String,
    to: String,
}

async fn recipe_diff(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<DiffQuery>,
) -> Response {
    let idx = state.index.read().await;
    let slug = Slug::from_base(&slug);
    let Some(family) = idx.families.get(&slug) else {
        return not_found_response(&state, &format!("No recipe with slug `{slug}`"));
    };
    let Some(from_key) = VersionKey::parse(&q.from) else {
        return not_found_response(&state, &format!("Bad `from` version `{}`", q.from));
    };
    let Some(to_key) = VersionKey::parse(&q.to) else {
        return not_found_response(&state, &format!("Bad `to` version `{}`", q.to));
    };
    let Some(from_v) = family.versions.iter().find(|v| v.key == from_key) else {
        return not_found_response(&state, &format!("Version {from_key} not found"));
    };
    let Some(to_v) = family.versions.iter().find(|v| v.key == to_key) else {
        return not_found_response(&state, &format!("Version {to_key} not found"));
    };
    let lines = crate::diff::unified_diff(&from_v.source, &to_v.source);
    let html = crate::diff::render_html(&lines);
    Html(render::diff_page(
        layout(&state, &family.title),
        family,
        from_key,
        to_key,
        &html,
    ))
    .into_response()
}

#[derive(serde::Serialize)]
struct HealthPayload {
    recipe_count: usize,
    family_count: usize,
    last_built_unix: Option<u64>,
    parse_errors: Vec<HealthError>,
    warnings: Vec<HealthError>,
}

#[derive(serde::Serialize)]
struct HealthError {
    path: String,
    message: String,
}

async fn health(State(state): State<AppState>) -> Response {
    let idx = state.index.read().await;
    let Some(t) = idx.last_built else {
        return (StatusCode::SERVICE_UNAVAILABLE, "index not built").into_response();
    };
    let payload = HealthPayload {
        recipe_count: idx.recipe_count(),
        family_count: idx.family_count(),
        last_built_unix: Some(
            t.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default(),
        ),
        parse_errors: idx
            .errors
            .iter()
            .map(|e| HealthError {
                path: e.path.display().to_string(),
                message: e.message.clone(),
            })
            .collect(),
        warnings: idx
            .warnings
            .iter()
            .map(|w| HealthError {
                path: w.path.display().to_string(),
                message: w.message.clone(),
            })
            .collect(),
    };
    axum::Json(payload).into_response()
}

async fn static_asset(Path(path): Path<String>) -> Response {
    match assets::Static::get(&path) {
        Some(content) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn not_found_response(state: &AppState, what: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(render::not_found(layout(state, "Not found"), what)),
    )
        .into_response()
}

// ---- shopping ----

/// `POST /shopping` accepts form-encoded `slugs[]=foo&slugs[]=bar@v1` and
/// `servings[<slug>]=N`. We don't care if the encoded keys collide on `slugs[]`
/// — `serde_urlencoded` collapses to the last value, so we parse the raw body
/// ourselves.
async fn shopping_create(State(state): State<AppState>, body: String) -> Response {
    let parsed = parse_shopping_form(&body);
    if parsed.slugs.is_empty() {
        return not_found_response(&state, "No recipes selected.");
    }

    let mut selections: Vec<Selection> = parsed
        .slugs
        .iter()
        .map(|s| shopping::parse_selection(s))
        .collect();
    for sel in &mut selections {
        if let Some(s) = parsed.servings.get(&sel.slug) {
            sel.override_servings = Some(*s);
        }
    }

    let token = state.lists.put(selections);
    Redirect::to(&state_url(&state, &format!("/shopping/{token}"))).into_response()
}

#[derive(Default)]
struct ShoppingForm {
    slugs: Vec<String>,
    servings: HashMap<String, u32>,
}

fn parse_shopping_form(body: &str) -> ShoppingForm {
    let mut out = ShoppingForm::default();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => continue,
        };
        let key = percent_decode(k);
        let val = percent_decode(v);
        if key == "slugs[]" || key == "slugs" {
            if !val.is_empty() {
                out.slugs.push(val);
            }
        } else if let Some(slug) = key
            .strip_prefix("servings[")
            .and_then(|s| s.strip_suffix(']'))
        {
            if let Ok(n) = val.parse::<u32>() {
                if n > 0 {
                    out.servings.insert(slug.to_string(), n);
                }
            }
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(serde::Deserialize)]
struct ShoppingViewQuery {
    #[serde(default)]
    format: Option<String>,
}

async fn shopping_view(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<ShoppingViewQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some(stored) = state.lists.get(&token) else {
        return not_found_response(
            &state,
            "This shopping list has expired. Generate it again from the home page.",
        );
    };
    let idx = state.index.read().await;
    let list = shopping::aggregate(&idx, &stored.selections);

    let format = q
        .format
        .as_deref()
        .and_then(Format::parse)
        .unwrap_or(Format::Html);

    if matches!(format, Format::Text) {
        let absolute_base = absolute_base_url(&state, &headers);
        let opts = shopping::render::RenderOpts {
            recipe_link_base: Some(absolute_base.as_str()),
        };
        let body = shopping::render::render_with(&list, format, &opts);
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, format.content_type().to_string())],
            body,
        )
            .into_response();
    }

    let absolute_base = absolute_base_url(&state, &headers);
    Html(render::shopping_page(
        layout(&state, "Shopping list"),
        &list,
        &token,
        &absolute_base,
    ))
    .into_response()
}

/// Resolve the absolute URL prefix used for outgoing recipe links.
/// `--public-url` (set at startup) wins; otherwise we derive it from
/// request headers, honouring `X-Forwarded-Proto` / `X-Forwarded-Host` so
/// the right value comes through behind a reverse proxy.
fn absolute_base_url(state: &AppState, headers: &axum::http::HeaderMap) -> String {
    if let Some(public) = state.public_url.as_ref().as_deref() {
        return public.to_string();
    }
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "http".to_string());
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "localhost".to_string());
    let bp = state.base_path.trim_end_matches('/');
    format!("{scheme}://{host}{bp}")
}

fn state_url(state: &AppState, suffix: &str) -> String {
    let bp = state.base_path.trim_end_matches('/');
    if suffix.starts_with('/') {
        format!("{bp}{suffix}")
    } else {
        format!("{bp}/{suffix}")
    }
}
