//! Middlewares HTTP d'authentification : résolution de la session depuis le
//! cookie, puis contrôles d'accès par route.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{CONTENT_TYPE, COOKIE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::{COOKIE_NAME, hash_token};
use crate::store::{Store, User};

/// Utilisateur authentifié, attaché aux extensions de la requête par
/// [`resolve_session`].
#[derive(Debug, Clone)]
pub struct CurrentUser(pub User);

/// Résout la session à partir du cookie `silo_session` et attache
/// l'utilisateur à la requête. Ne bloque pas les requêtes anonymes : c'est
/// le rôle de [`require_auth`].
pub async fn resolve_session(
    State(store): State<Arc<Store>>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(token) = session_token(req.headers()) {
        if let Ok(user) = store.get_session_user(&hash_token(&token)) {
            req.extensions_mut().insert(CurrentUser(user));
        }
    }
    next.run(req).await
}

/// Rejette avec 401 toute requête sans utilisateur authentifié.
pub async fn require_auth(req: Request, next: Next) -> Response {
    if req.extensions().get::<CurrentUser>().is_none() {
        return plain_error(StatusCode::UNAUTHORIZED, "authentification requise");
    }
    next.run(req).await
}

/// Rejette avec 403 toute requête dont l'utilisateur n'a pas le rôle admin.
/// Doit être placé derrière [`require_auth`].
pub async fn require_admin(req: Request, next: Next) -> Response {
    match req.extensions().get::<CurrentUser>() {
        Some(CurrentUser(user)) if user.role == "admin" => next.run(req).await,
        _ => plain_error(StatusCode::FORBIDDEN, "droits administrateur requis"),
    }
}

/// Valeur du cookie de session, s'il est présent et non vide. Seul le
/// premier cookie portant ce nom est pris en compte.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|h| h.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, value)| value)?;
    (!value.is_empty()).then(|| value.to_string())
}

/// En-tête `Set-Cookie` du cookie de session. `max_age <= 0` supprime le
/// cookie côté navigateur.
pub fn session_cookie(value: &str, max_age: i64, secure: bool) -> String {
    let mut cookie = format!("{COOKIE_NAME}={value}; Path=/; Max-Age={}", max_age.max(0));
    cookie.push_str("; HttpOnly");
    if secure {
        cookie.push_str("; Secure");
    }
    cookie.push_str("; SameSite=Strict");
    cookie
}

// Le corps est un JSON servi en text/plain : c'est la réponse historique
// de ces deux contrôles (http.Error en Go), que le frontend sait lire.
fn plain_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [
            (CONTENT_TYPE, "text/plain; charset=utf-8"),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        format!("{{\"error\":\"{message}\"}}\n"),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn headers(cookies: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for c in cookies {
            h.append(COOKIE, HeaderValue::from_str(c).unwrap());
        }
        h
    }

    #[test]
    fn test_session_token_parmi_plusieurs_cookies_trouve() {
        let h = headers(&["theme=dark; silo_session=abc123; autre=x"]);
        assert_eq!(session_token(&h).as_deref(), Some("abc123"));
    }

    #[test]
    fn test_session_token_absent_ou_vide_retourne_none() {
        assert_eq!(session_token(&headers(&[])), None);
        assert_eq!(session_token(&headers(&["theme=dark"])), None);
        assert_eq!(session_token(&headers(&["silo_session="])), None);
    }

    #[test]
    fn test_session_token_plusieurs_en_tetes_cookie_trouve() {
        let h = headers(&["theme=dark", "silo_session=xyz"]);
        assert_eq!(session_token(&h).as_deref(), Some("xyz"));
    }

    #[test]
    fn test_session_cookie_attributs_de_securite_presents() {
        assert_eq!(
            session_cookie("tok", 3600, true),
            "silo_session=tok; Path=/; Max-Age=3600; HttpOnly; Secure; SameSite=Strict"
        );
        assert_eq!(
            session_cookie("", -1, false),
            "silo_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict"
        );
    }
}
