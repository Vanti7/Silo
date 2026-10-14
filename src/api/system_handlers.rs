use axum::http::StatusCode;
use axum::response::Response;

use super::{write_error, write_json};
use crate::system;

/// Instantané des métriques système courantes.
pub(super) async fn system_stats() -> Response {
    match system::collect().await {
        Ok(stats) => write_json(StatusCode::OK, &stats),
        Err(e) => {
            tracing::error!(err = %e, "collecte des métriques système");
            write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de lire les métriques système",
            )
        }
    }
}

/// Informations statiques de l'hôte.
pub(super) async fn system_info() -> Response {
    match system::collect_info() {
        Ok(info) => write_json(StatusCode::OK, &info),
        Err(e) => {
            tracing::error!(err = %e, "collecte des informations système");
            write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de lire les informations système",
            )
        }
    }
}
