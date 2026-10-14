use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::{CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::{IntoResponse, Response};
use futures_util::{StreamExt, stream};
use serde::Deserialize;

use super::{AppState, write_error, write_json};

/// Tous les containers, démarrés ou non.
pub(super) async fn list_containers(State(s): State<AppState>) -> Response {
    match s.docker.list_containers().await {
        Ok(c) => write_json(StatusCode::OK, &c),
        Err(e) => {
            tracing::error!(err = %e, "liste containers docker");
            write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de lister les containers",
            )
        }
    }
}

fn no_content_or<E: std::fmt::Display>(
    result: Result<(), E>,
    id: &str,
    log: &str,
    message: &str,
) -> Response {
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(id, err = %e, "{log}");
            write_error(StatusCode::INTERNAL_SERVER_ERROR, message)
        }
    }
}

/// Démarre un container.
pub(super) async fn start_container(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    no_content_or(
        s.docker.start_container(&id).await,
        &id,
        "démarrage container",
        "impossible de démarrer le container",
    )
}

/// Arrête un container.
pub(super) async fn stop_container(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    no_content_or(
        s.docker.stop_container(&id).await,
        &id,
        "arrêt container",
        "impossible d'arrêter le container",
    )
}

/// Redémarre un container.
pub(super) async fn restart_container(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    no_content_or(
        s.docker.restart_container(&id).await,
        &id,
        "redémarrage container",
        "impossible de redémarrer le container",
    )
}

#[derive(Deserialize)]
pub(super) struct RemoveQuery {
    #[serde(default)]
    force: String,
}

/// Supprime un container (`?force=true` pour un container en cours d'exécution).
pub(super) async fn remove_container(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RemoveQuery>,
) -> Response {
    no_content_or(
        s.docker.remove_container(&id, q.force == "true").await,
        &id,
        "suppression container",
        "impossible de supprimer le container",
    )
}

#[derive(Deserialize)]
pub(super) struct LogsQuery {
    #[serde(default)]
    follow: String,
    #[serde(default)]
    tail: String,
}

/// Logs d'un container, en flux continu (`?follow=true`) ou en lot borné
/// (`?tail=200` par défaut).
pub(super) async fn container_logs(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<LogsQuery>,
) -> Response {
    let tail = if q.tail.is_empty() { "200" } else { &q.tail };
    let failed = |e: &dyn std::fmt::Display| {
        tracing::error!(id, err = %e, "logs container");
        write_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "impossible de récupérer les logs",
        )
    };

    let mut logs = match s.docker.container_logs(&id, q.follow == "true", tail).await {
        Ok(l) => l,
        Err(e) => return failed(&e),
    };
    // La requête vers le daemon ne part qu'à la première lecture : on la
    // fait avant de répondre pour qu'un container inconnu donne une erreur
    // HTTP plutôt qu'un 200 vide.
    let first = match logs.next_chunk().await {
        Some(Err(e)) => return failed(&e),
        Some(Ok(chunk)) => Some(chunk),
        None => None,
    };
    let rest = stream::unfold(logs, |mut logs| async move {
        match logs.next_chunk().await {
            Some(Ok(chunk)) => Some((chunk, logs)),
            // Une coupure en cours de flux termine simplement la réponse.
            _ => None,
        }
    });
    let body = stream::iter(first).chain(rest).map(Ok::<Bytes, Infallible>);

    (
        StatusCode::OK,
        [
            (CONTENT_TYPE, "text/plain; charset=utf-8"),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}

/// Images Docker locales.
pub(super) async fn list_images(State(s): State<AppState>) -> Response {
    match s.docker.list_images().await {
        Ok(i) => write_json(StatusCode::OK, &i),
        Err(e) => {
            tracing::error!(err = %e, "liste images docker");
            write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de lister les images",
            )
        }
    }
}

/// Résumé de l'état du daemon Docker.
pub(super) async fn docker_info(State(s): State<AppState>) -> Response {
    match s.docker.get_info().await {
        Ok(i) => write_json(StatusCode::OK, &i),
        Err(e) => {
            tracing::error!(err = %e, "info daemon docker");
            write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de contacter le daemon Docker",
            )
        }
    }
}
