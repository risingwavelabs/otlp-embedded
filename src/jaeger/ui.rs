use std::cmp::Reverse;

use axum::{
    extract::{Path, Query},
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use itertools::Itertools;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;

use crate::StateRef;

// TODO: make `base_path` optional.
pub fn app(state: StateRef, base_path: &str) -> Router {
    if !base_path.starts_with('/') || !base_path.ends_with('/') {
        panic!("base_path must start and end with /");
    }
    let base_tag = format!(r#"<base href="{base_path}""#);

    Router::new()
        .route("/api/traces/:hex_id", get(trace))
        .route("/api/services", get(services))
        .route("/api/services/:service/operations", get(operations))
        .route("/api/traces", get(traces))
        .layer(Extension(state))
        .fallback(|uri| async move { static_handler(uri, &base_tag).await })
}

async fn trace(
    Path(hex_id): Path<String>,
    Extension(state): Extension<StateRef>,
) -> impl IntoResponse {
    let id = hex::decode(&hex_id).unwrap_or_default();
    let trace = state.read().await.get_by_id(&id);

    if let Some(trace) = trace {
        Json(trace.to_jaeger()).into_response()
    } else {
        not_found_with_msg(format!("Trace {hex_id} not found, maybe expired."))
    }
}

async fn services() -> impl IntoResponse {
    let mock = json!({
        "data": ["all"],
        "total": 1,
    });

    Json(mock).into_response()
}

async fn operations() -> impl IntoResponse {
    let mock = json!({
        "data": [],
        "total": 0,
    });

    Json(mock).into_response()
}

#[derive(Deserialize)]
struct TracesQuery {
    limit: usize,
}

async fn traces(
    Query(query): Query<TracesQuery>,
    Extension(state): Extension<StateRef>,
) -> impl IntoResponse {
    let traces = (state.read().await)
        .get_all_complete()
        .into_iter()
        .sorted_by_cached_key(|t| Reverse(t.end_time))
        .map(|t| t.to_jaeger_entry())
        .take(query.limit)
        .collect_vec();

    let mock = json!({
        "data": traces,
        "total": traces.len(),
    });

    Json(mock).into_response()
}

const INDEX_HTML: &str = "index.html";

#[derive(RustEmbed)]
#[folder = "jaeger-ui/build"]
struct Assets;

async fn static_handler(uri: Uri, base_tag: &str) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path == INDEX_HTML {
        return index_html(base_tag);
    }

    match Assets::get(path) {
        Some(file) => {
            let mime = file.metadata.mimetype();

            let mut res = file.data.into_response();
            res.headers_mut()
                .insert(header::CONTENT_TYPE, mime.parse().unwrap());
            res
        }

        None => {
            if path.starts_with("api") || path.starts_with("static") {
                // For those routes, we simply return 404.
                not_found()
            } else {
                // Due to the frontend is a SPA (Single Page Application),
                // it has own frontend routes, we should return the ROOT PAGE
                // to avoid frontend route 404.
                (StatusCode::TEMPORARY_REDIRECT, index_html(base_tag)).into_response()
            }
        }
    }
}

fn index_html(base_tag: &str) -> Response {
    let file = Assets::get(INDEX_HTML).unwrap();
    let data = std::str::from_utf8(&file.data)
        .unwrap()
        .replace(r#"<base href="/""#, base_tag);

    Html(data).into_response()
}

fn not_found() -> Response {
    not_found_with_msg("Not Found")
}

fn not_found_with_msg(msg: impl Into<String>) -> Response {
    (StatusCode::NOT_FOUND, msg.into()).into_response()
}
