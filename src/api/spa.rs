use std::borrow::Cow;

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, LOCATION};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::web::Dist;

/// Fichiers statiques du frontend, avec repli sur `index.html` pour toute
/// route inconnue, afin que le routage côté client (react-router) survive
/// à un rechargement de page.
pub(super) async fn spa(uri: Uri) -> Response {
    let path = uri.path();
    // URL canonique de l'index, comme le faisait le serveur de fichiers Go.
    if path == "/index.html" {
        return (StatusCode::MOVED_PERMANENTLY, [(LOCATION, "./")]).into_response();
    }

    let requested = path.trim_start_matches('/');
    let candidate = if requested.is_empty() || requested.split('/').any(|seg| seg == "..") {
        "index.html"
    } else {
        requested
    };

    match Dist::get(candidate)
        .map(|f| (candidate, f))
        .or_else(|| Dist::get("index.html").map(|f| ("index.html", f)))
    {
        Some((name, file)) => {
            let body = match file.data {
                Cow::Borrowed(b) => Body::from(b),
                Cow::Owned(v) => Body::from(v),
            };
            (
                [(
                    CONTENT_TYPE,
                    mime_guess::from_path(name)
                        .first_or_octet_stream()
                        .to_string(),
                )],
                body,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
