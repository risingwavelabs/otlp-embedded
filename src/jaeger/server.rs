use axum::{
    extract::Path,
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use rust_embed::RustEmbed;

use crate::StateRef;

pub async fn run(state: StateRef) {
    let app = Router::new()
        .route("/api/traces/:id", get(trace))
        .layer(Extension(state))
        .fallback(static_handler);

    axum::Server::bind(&"0.0.0.0:10188".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn trace(Path(id): Path<String>, Extension(state): Extension<StateRef>) -> impl IntoResponse {
    let id = hex::decode(id).unwrap_or_default();
    let trace = state.read().await.get_by_id(&id);

    if let Some(trace) = trace {
        Json(trace.to_jaeger()).into_response()
    } else {
        not_found()
    }
}

const INDEX_HTML: &str = "index.html";

#[derive(RustEmbed)]
#[folder = "jaeger-ui/build"]
struct Assets;

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    match Assets::get(path) {
        Some(content) => {
            let mime = content.metadata.mimetype();

            let mut res = content.data.into_response();
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
                (StatusCode::TEMPORARY_REDIRECT, index_html()).into_response()
            }
        }
    }
}

fn index_html() -> Response {
    let content = Assets::get(INDEX_HTML).unwrap();
    Html(content.data).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}
