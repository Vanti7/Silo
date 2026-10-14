use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::auth_handlers::{
    UserResponse, hash_password, internal_error, valid_password, valid_username,
};
use super::{AppState, Server, decode_json, write_error, write_json};
use crate::store::StoreError;

/// Tous les comptes utilisateurs.
pub(super) async fn list_users(State(s): State<AppState>) -> Response {
    match s.store.list_users() {
        Ok(users) => write_json(
            StatusCode::OK,
            &users.iter().map(UserResponse::from).collect::<Vec<_>>(),
        ),
        Err(_) => internal_error(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateUserRequest {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    role: String,
}

/// Crée un utilisateur (rôle `admin` ou `user`).
pub(super) async fn create_user(State(s): State<AppState>, body: Bytes) -> Response {
    let Ok(req) = decode_json::<CreateUserRequest>(&body) else {
        return write_error(StatusCode::BAD_REQUEST, "requête invalide");
    };
    if !valid_username(&req.username) || !valid_password(&req.password) {
        return write_error(
            StatusCode::BAD_REQUEST,
            "identifiant ou mot de passe invalide (mot de passe : 8 caractères minimum)",
        );
    }
    if !valid_role(&req.role) {
        return write_error(StatusCode::BAD_REQUEST, "rôle invalide (admin ou user)");
    }

    let Ok(hash) = hash_password(req.password).await else {
        return internal_error();
    };
    let id = match s.store.create_user(&req.username, &hash, &req.role) {
        Ok(id) => id,
        Err(StoreError::AlreadyExists) => {
            return write_error(StatusCode::CONFLICT, "cet identifiant existe déjà");
        }
        Err(_) => return internal_error(),
    };
    match s.store.get_user_by_id(id) {
        Ok(user) => write_json(StatusCode::CREATED, &UserResponse::from(&user)),
        Err(_) => internal_error(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateUserRequest {
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    role: Option<String>,
}

/// Modifie le mot de passe et/ou le rôle d'un utilisateur.
pub(super) async fn update_user(
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(id) = id.parse::<i64>() else {
        return write_error(StatusCode::BAD_REQUEST, "identifiant invalide");
    };
    let Ok(req) = decode_json::<UpdateUserRequest>(&body) else {
        return write_error(StatusCode::BAD_REQUEST, "requête invalide");
    };

    if let Some(role) = &req.role {
        if !valid_role(role) {
            return write_error(StatusCode::BAD_REQUEST, "rôle invalide (admin ou user)");
        }
        if role != "admin" {
            if let Err(message) = ensure_not_last_admin(&s, id) {
                return write_error(StatusCode::CONFLICT, &message);
            }
        }
        if let Err(e) = s.store.update_user_role(id, role) {
            return write_store_err(&e);
        }
    }

    if let Some(password) = req.password {
        if !valid_password(&password) {
            return write_error(
                StatusCode::BAD_REQUEST,
                "mot de passe invalide (8 caractères minimum)",
            );
        }
        let Ok(hash) = hash_password(password).await else {
            return internal_error();
        };
        if let Err(e) = s.store.update_user_password(id, &hash) {
            return write_store_err(&e);
        }
        // Réinitialisation par un administrateur : toutes les sessions du
        // compte sont révoquées, y compris celle de l'administrateur s'il
        // réinitialise son propre compte par ce biais.
        if let Err(e) = s.store.delete_user_sessions(id) {
            tracing::error!(user = id, err = %e, "révocation des sessions après réinitialisation");
            return internal_error();
        }
    }

    match s.store.get_user_by_id(id) {
        Ok(user) => write_json(StatusCode::OK, &UserResponse::from(&user)),
        Err(e) => write_store_err(&e),
    }
}

/// Supprime un compte, sauf s'il s'agit du dernier administrateur.
pub(super) async fn delete_user(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    let Ok(id) = id.parse::<i64>() else {
        return write_error(StatusCode::BAD_REQUEST, "identifiant invalide");
    };
    if let Err(message) = ensure_not_last_admin(&s, id) {
        return write_error(StatusCode::CONFLICT, &message);
    }
    match s.store.delete_user(id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => write_store_err(&e),
    }
}

/// Refuse de retirer le dernier administrateur, pour ne jamais se retrouver
/// sans accès admin. Le message d'erreur est renvoyé tel quel au client.
fn ensure_not_last_admin(s: &Server, id: i64) -> Result<(), String> {
    let target = s.store.get_user_by_id(id).map_err(|e| e.to_string())?;
    if target.role != "admin" {
        return Ok(());
    }
    if s.store.count_admins().map_err(|e| e.to_string())? <= 1 {
        return Err(
            "impossible de supprimer ou rétrograder le dernier compte administrateur".to_string(),
        );
    }
    Ok(())
}

fn valid_role(role: &str) -> bool {
    role == "admin" || role == "user"
}

pub(super) fn write_store_err(e: &StoreError) -> Response {
    match e {
        StoreError::NotFound => write_error(StatusCode::NOT_FOUND, "utilisateur introuvable"),
        _ => internal_error(),
    }
}
