//! API HTTP REST de silo (JSON sous `/api/v1`) et service du frontend
//! embarqué.

mod auth_handlers;
mod docker_handlers;
mod download;
mod files_handlers;
mod router;
mod spa;
mod storage_handlers;
mod system_handlers;
mod users_handlers;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::Config;
use crate::store::Store;
use crate::{docker, files};

pub use router::new_router;

/// Dépendances partagées par les handlers HTTP.
pub struct Server {
    pub config: Config,
    pub store: Arc<Store>,
    pub docker: docker::Client,
    pub files: files::Manager,
}

type AppState = Arc<Server>;

/// Réponse JSON avec le code de statut donné.
fn write_json<T: Serialize>(status: StatusCode, v: &T) -> Response {
    let mut body = serde_json::to_vec(v).expect("types de réponse sérialisables en JSON");
    body.push(b'\n');
    (
        status,
        [(CONTENT_TYPE, "application/json; charset=utf-8")],
        body,
    )
        .into_response()
}

/// Réponse d'erreur JSON standardisée `{"error": "..."}`.
fn write_error(status: StatusCode, message: &str) -> Response {
    write_json(status, &serde_json::json!({ "error": message }))
}

/// Décode un corps JSON. Seule la première valeur est lue (le reste du
/// corps est ignoré) ; les champs inconnus sont refusés par les types cibles.
fn decode_json<T: DeserializeOwned>(body: &[u8]) -> Result<T, serde_json::Error> {
    T::deserialize(&mut serde_json::Deserializer::from_slice(body))
}

/// Exécute un traitement bloquant (bcrypt, accès disque) hors des threads
/// de l'exécuteur asynchrone, pour ne pas figer les autres requêtes.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(v) => v,
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}
