use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{MethodRouter, delete, get, patch, post};

use super::auth_handlers::{change_password, login, logout, me, setup, setup_status};
use super::docker_handlers::{
    container_logs, docker_info, list_containers, list_images, remove_container, restart_container,
    start_container, stop_container,
};
use super::files_handlers::{
    UPLOAD_LIMIT, delete_file, download_file, list_dir, list_roots, mkdir, rename_file, upload_file,
};
use super::spa::spa;
use super::storage_handlers::{
    filesystem_usage, list_filesystems, list_subvolumes, scrub_status, start_scrub,
};
use super::system_handlers::{system_info, system_stats};
use super::users_handlers::{create_user, delete_user, list_users, update_user};
use super::{AppState, Server};
use crate::auth::middleware::{require_admin, require_auth, resolve_session};

type Route = MethodRouter<AppState>;

/// Accessible à tout utilisateur authentifié.
fn authed(route: Route) -> Route {
    route.route_layer(from_fn(require_auth))
}

/// Réservé aux administrateurs. La dernière couche ajoutée s'exécute en
/// premier : une requête anonyme reçoit donc 401, et non 403.
fn admin(route: Route) -> Route {
    route
        .route_layer(from_fn(require_admin))
        .route_layer(from_fn(require_auth))
}

/// Routeur complet : API JSON sous `/api/v1` (chaque route déclare son
/// niveau de protection) et frontend statique pour tout le reste.
pub fn new_router(server: std::sync::Arc<Server>) -> Router {
    let store = server.store.clone();
    Router::new()
        // -- Setup et authentification (non protégés) --
        .route("/api/v1/setup/status", get(setup_status))
        .route("/api/v1/setup", post(setup))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
        .route("/api/v1/auth/password", authed(patch(change_password)))
        // -- Système --
        .route("/api/v1/system/stats", authed(get(system_stats)))
        .route("/api/v1/system/info", authed(get(system_info)))
        // -- Stockage btrfs --
        .route("/api/v1/storage/filesystems", authed(get(list_filesystems)))
        .route("/api/v1/storage/usage", authed(get(filesystem_usage)))
        .route("/api/v1/storage/subvolumes", authed(get(list_subvolumes)))
        .route(
            "/api/v1/storage/scrub",
            authed(get(scrub_status)).merge(admin(post(start_scrub))),
        )
        // -- Explorateur de fichiers --
        .route("/api/v1/files/roots", authed(get(list_roots)))
        .route("/api/v1/files/list", authed(get(list_dir)))
        .route("/api/v1/files/download", authed(get(download_file)))
        .route("/api/v1/files/mkdir", admin(post(mkdir)))
        .route(
            "/api/v1/files/upload",
            admin(post(upload_file).layer(DefaultBodyLimit::max(UPLOAD_LIMIT))),
        )
        .route("/api/v1/files/rename", admin(post(rename_file)))
        .route("/api/v1/files", admin(delete(delete_file)))
        // -- Docker --
        .route("/api/v1/docker/containers", authed(get(list_containers)))
        .route(
            "/api/v1/docker/containers/{id}/logs",
            authed(get(container_logs)),
        )
        .route("/api/v1/docker/images", authed(get(list_images)))
        .route("/api/v1/docker/info", authed(get(docker_info)))
        .route(
            "/api/v1/docker/containers/{id}/start",
            admin(post(start_container)),
        )
        .route(
            "/api/v1/docker/containers/{id}/stop",
            admin(post(stop_container)),
        )
        .route(
            "/api/v1/docker/containers/{id}/restart",
            admin(post(restart_container)),
        )
        .route(
            "/api/v1/docker/containers/{id}",
            admin(delete(remove_container)),
        )
        // -- Utilisateurs (réservé admin) --
        .route("/api/v1/users", admin(get(list_users).post(create_user)))
        .route(
            "/api/v1/users/{id}",
            admin(patch(update_user).delete(delete_user)),
        )
        // -- Frontend statique (SPA) --
        .fallback(spa)
        .layer(from_fn_with_state(store, resolve_session))
        .with_state(server)
}
