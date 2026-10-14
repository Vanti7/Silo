use axum::Extension;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::users_handlers::write_store_err;
use super::{AppState, Server, blocking, decode_json, write_error, write_json};
use crate::auth::middleware::{CurrentUser, session_cookie, session_token};
use crate::auth::{self, hash_token};
use crate::store::{StoreError, User};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
}

#[derive(Serialize)]
pub(super) struct UserResponse {
    id: i64,
    username: String,
    role: String,
}

impl From<&User> for UserResponse {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            username: u.username.clone(),
            role: u.role.clone(),
        }
    }
}

/// Indique si le premier compte administrateur reste à créer.
pub(super) async fn setup_status(State(s): State<AppState>) -> Response {
    match s.store.count_users() {
        Ok(n) => write_json(StatusCode::OK, &serde_json::json!({ "needsSetup": n == 0 })),
        Err(_) => write_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "impossible de vérifier l'état d'initialisation",
        ),
    }
}

/// Crée le premier compte administrateur. Refusé dès qu'un utilisateur
/// existe, ce qui interdit toute création ultérieure non authentifiée.
pub(super) async fn setup(State(s): State<AppState>, body: Bytes) -> Response {
    match s.store.count_users() {
        Ok(0) => {}
        Ok(_) => {
            return write_error(StatusCode::CONFLICT, "un compte administrateur existe déjà");
        }
        Err(_) => {
            return write_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "impossible de vérifier l'état d'initialisation",
            );
        }
    }

    let Ok(creds) = decode_json::<Credentials>(&body) else {
        return write_error(StatusCode::BAD_REQUEST, "requête invalide");
    };
    if !valid_username(&creds.username) || !valid_password(&creds.password) {
        return write_error(
            StatusCode::BAD_REQUEST,
            "identifiant ou mot de passe invalide (mot de passe : 8 caractères minimum)",
        );
    }

    let Ok(hash) = hash_password(creds.password).await else {
        return internal_error();
    };
    let Ok(user_id) = s.store.create_user(&creds.username, &hash, "admin") else {
        return write_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "création du compte impossible",
        );
    };
    let Ok(user) = s.store.get_user_by_id(user_id) else {
        return internal_error();
    };
    let Ok(cookie) = start_session(&s, &user) else {
        return internal_error();
    };
    (
        [(SET_COOKIE, cookie)],
        write_json(StatusCode::CREATED, &UserResponse::from(&user)),
    )
        .into_response()
}

/// Authentifie un utilisateur existant et ouvre une session.
pub(super) async fn login(State(s): State<AppState>, body: Bytes) -> Response {
    let Ok(creds) = decode_json::<Credentials>(&body) else {
        return write_error(StatusCode::BAD_REQUEST, "requête invalide");
    };

    let user = match s.store.get_user_by_username(&creds.username) {
        Ok(u) => u,
        Err(StoreError::NotFound) => {
            return write_error(StatusCode::UNAUTHORIZED, "identifiants invalides");
        }
        Err(_) => return internal_error(),
    };
    if !verify_password(user.password_hash.clone(), creds.password).await {
        return write_error(StatusCode::UNAUTHORIZED, "identifiants invalides");
    }

    let Ok(cookie) = start_session(&s, &user) else {
        return internal_error();
    };
    (
        [(SET_COOKIE, cookie)],
        write_json(StatusCode::OK, &UserResponse::from(&user)),
    )
        .into_response()
}

/// Invalide la session courante.
pub(super) async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers) {
        // La session est de toute façon retirée côté navigateur : un échec
        // de suppression en base ne doit pas empêcher la déconnexion.
        let _ = s.store.delete_session(&hash_token(&token));
    }
    (
        StatusCode::NO_CONTENT,
        [(
            SET_COOKIE,
            session_cookie("", -1, s.config.session_cookie_secure),
        )],
    )
        .into_response()
}

/// Utilisateur authentifié courant.
pub(super) async fn me(user: Option<Extension<CurrentUser>>) -> Response {
    match user {
        Some(Extension(CurrentUser(u))) => write_json(StatusCode::OK, &UserResponse::from(&u)),
        None => write_error(StatusCode::UNAUTHORIZED, "authentification requise"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ChangePasswordRequest {
    #[serde(default)]
    current_password: String,
    #[serde(default)]
    new_password: String,
}

/// Changement de son propre mot de passe. Le mot de passe actuel est
/// exigé : un cookie de session volé ne suffit pas à s'approprier le compte.
pub(super) async fn change_password(
    State(s): State<AppState>,
    Extension(CurrentUser(user)): Extension<CurrentUser>,
    body: Bytes,
) -> Response {
    let Ok(req) = decode_json::<ChangePasswordRequest>(&body) else {
        return write_error(StatusCode::BAD_REQUEST, "requête invalide");
    };
    if !verify_password(user.password_hash.clone(), req.current_password).await {
        return write_error(StatusCode::UNAUTHORIZED, "mot de passe actuel incorrect");
    }
    if !valid_password(&req.new_password) {
        return write_error(
            StatusCode::BAD_REQUEST,
            "nouveau mot de passe invalide (8 caractères minimum)",
        );
    }

    let Ok(hash) = hash_password(req.new_password).await else {
        return internal_error();
    };
    if let Err(e) = s.store.update_user_password(user.id, &hash) {
        return write_store_err(&e);
    }

    // Toutes les sessions du compte sont révoquées, puis une nouvelle est
    // ouverte pour ce client : il reste connecté ici, les autres non.
    if let Err(e) = s.store.delete_user_sessions(user.id) {
        tracing::error!(user = user.id, err = %e, "révocation des sessions après changement de mot de passe");
        return internal_error();
    }
    let Ok(cookie) = start_session(&s, &user) else {
        return internal_error();
    };
    (StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response()
}

/// Génère un jeton, persiste son empreinte et retourne l'en-tête `Set-Cookie`.
fn start_session(s: &Server, user: &User) -> anyhow::Result<String> {
    let token = auth::new_session_token()?;
    let ttl = time::Duration::hours(s.config.session_ttl_hours);
    s.store.create_session(&hash_token(&token), user.id, ttl)?;
    Ok(session_cookie(
        &token,
        ttl.whole_seconds(),
        s.config.session_cookie_secure,
    ))
}

pub(super) async fn hash_password(password: String) -> Result<String, auth::AuthError> {
    blocking(move || auth::hash_password(&password)).await
}

async fn verify_password(hash: String, password: String) -> bool {
    blocking(move || auth::verify_password(&hash, &password)).await
}

pub(super) fn valid_username(u: &str) -> bool {
    (3..=64).contains(&u.len())
}

pub(super) fn valid_password(p: &str) -> bool {
    auth::validate_password(p).is_ok()
}

pub(super) fn internal_error() -> Response {
    write_error(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne")
}
