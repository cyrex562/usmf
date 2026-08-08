//! Serves the built Vue SPA from bytes embedded into the binary at compile
//! time, so a release build is a single self-contained executable. Only
//! compiled in with `--features serve-frontend`; requires `frontend/dist` to
//! exist at build time (run `npm run build` in `frontend/` first).

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../../frontend/dist/"]
struct Assets;

/// Serves `path` from the embedded bundle if present; otherwise falls back to
/// `index.html` so vue-router's history-mode client-side routes (e.g.
/// `/units`, entered directly rather than via in-app navigation) resolve to
/// the SPA instead of a 404.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    serve(path)
        .unwrap_or_else(|| serve("index.html").unwrap_or(StatusCode::NOT_FOUND.into_response()))
}

fn serve(path: &str) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };
    let file = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        (
            [(header::CONTENT_TYPE, mime.as_ref().to_string())],
            file.data,
        )
            .into_response(),
    )
}
