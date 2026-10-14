use axum::body::Bytes;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::{decode_json, write_error, write_json};
use crate::btrfs;

#[derive(Deserialize)]
pub(super) struct MountPointQuery {
    #[serde(default)]
    path: String,
}

fn missing_mount_point() -> Response {
    write_error(
        StatusCode::BAD_REQUEST,
        "paramètre 'path' (point de montage) requis",
    )
}

fn json_or_500<T: Serialize, E: std::fmt::Display>(
    result: Result<T, E>,
    path: &str,
    log: &str,
    message: &str,
) -> Response {
    match result {
        Ok(v) => write_json(StatusCode::OK, &v),
        Err(e) => {
            tracing::error!(path, err = %e, "{log}");
            write_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

/// Systèmes de fichiers btrfs détectés.
pub(super) async fn list_filesystems() -> Response {
    json_or_500(
        btrfs::list_filesystems().await,
        "",
        "liste systèmes de fichiers btrfs",
        "impossible de lire les systèmes de fichiers btrfs",
    )
}

/// Répartition d'allocation d'un système de fichiers (`?path=/mnt/pool`).
pub(super) async fn filesystem_usage(Query(q): Query<MountPointQuery>) -> Response {
    if q.path.is_empty() {
        return missing_mount_point();
    }
    json_or_500(
        btrfs::get_usage(&q.path).await,
        &q.path,
        "usage btrfs",
        "impossible de lire l'usage du système de fichiers",
    )
}

/// Sous-volumes d'un système de fichiers (`?path=/mnt/pool`).
pub(super) async fn list_subvolumes(Query(q): Query<MountPointQuery>) -> Response {
    if q.path.is_empty() {
        return missing_mount_point();
    }
    json_or_500(
        btrfs::list_subvolumes(&q.path).await,
        &q.path,
        "liste sous-volumes btrfs",
        "impossible de lire les sous-volumes",
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScrubRequest {
    #[serde(default)]
    path: String,
}

/// Démarre un scrub sur le point de montage donné (`{"path": "/mnt/pool"}`).
pub(super) async fn start_scrub(body: Bytes) -> Response {
    let req = match decode_json::<ScrubRequest>(&body) {
        Ok(r) if !r.path.is_empty() => r,
        _ => return missing_mount_point(),
    };
    match btrfs::start_scrub(&req.path).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(e) => {
            tracing::error!(path = req.path, err = %e, "démarrage scrub btrfs");
            write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de démarrer le scrub",
            )
        }
    }
}

/// État du dernier scrub (`?path=/mnt/pool`).
pub(super) async fn scrub_status(Query(q): Query<MountPointQuery>) -> Response {
    if q.path.is_empty() {
        return missing_mount_point();
    }
    json_or_500(
        btrfs::get_scrub_status(&q.path).await,
        &q.path,
        "statut scrub btrfs",
        "impossible de lire le statut du scrub",
    )
}
