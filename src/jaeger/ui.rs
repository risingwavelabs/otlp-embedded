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
/// Create a new [`axum::Router`] for the Jaeger UI to visualize the traces
/// stored in the given [`StateRef`].
///
/// The `base_path` is used for the application to load static assets correctly.
/// It should start and end with `/`. For example,
///
/// - if the application is served at `http://localhost:3000/`, then `base_path`
///   should be `/`.
/// - if the application is served at `http://localhost:3000/trace/`, then
///   `base_path` should be `/trace/`.
pub fn app(state: StateRef, base_path: &str) -> Router {
    if !base_path.starts_with('/') || !base_path.ends_with('/') {
        panic!("base_path must start and end with /");
    }
    let base_tag = format!(r#"<base href="{base_path}""#);

    let api = Router::new()
        .route("/traces/{hex_id}", get(trace))
        .route("/services", get(services))
        .route("/services/{service}/operations", get(operations))
        .route("/traces", get(traces))
        .layer(Extension(state))
        .fallback(|_: Uri| async move { not_found_with_msg("API not supported") });

    Router::new()
        .nest("/api/", api)
        .fallback(|uri| async move { static_handler(uri, &base_tag).await })
}

async fn trace(
    Path(hex_id): Path<String>,
    Extension(state): Extension<StateRef>,
) -> impl IntoResponse {
    let id = hex::decode(&hex_id).unwrap_or_default();
    let trace = state.write().await.get_by_id(&id);

    if let Some(trace) = trace {
        Json(trace.to_jaeger_batch()).into_response()
    } else {
        not_found_with_msg(format!("Trace {hex_id} not found, maybe expired."))
    }
}

async fn services(Extension(state): Extension<StateRef>) -> impl IntoResponse {
    let state = state.read().await;
    let all_services = state.get_all_services();
    let len = all_services.len();

    let res = json!({
        "data": all_services,
        "total": len,
    });

    Json(res).into_response()
}

async fn operations(
    Path(service): Path<String>,
    Extension(state): Extension<StateRef>,
) -> impl IntoResponse {
    let state = state.read().await;
    let operations = state.get_operations(&service);
    let len = operations.len();

    let res = json!({
        "data": operations,
        "total": len,
    });

    Json(res).into_response()
}

#[derive(Deserialize)]
struct TracesQuery {
    service: Option<String>,
    operation: Option<String>,
    limit: usize,
}

async fn traces(
    Query(TracesQuery {
        service,
        operation,
        limit,
    }): Query<TracesQuery>,
    Extension(state): Extension<StateRef>,
) -> impl IntoResponse {
    let traces = (state.read().await)
        .get_all_complete()
        .filter(|t| {
            if let Some(service) = &service {
                t.service_name().unwrap() == service
            } else {
                true
            }
        })
        .filter(|t| {
            if let Some(operation) = &operation {
                t.operation().unwrap() == operation
            } else {
                true
            }
        })
        .sorted_by_cached_key(|t| Reverse(t.end_time))
        .map(|t| t.to_jaeger())
        .take(limit)
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
            if path.starts_with("static") {
                // For inexistent static assets, we simply return 404.
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
