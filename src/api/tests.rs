//! Tests de bout en bout du routeur : requêtes HTTP réelles sur l'API,
//! base SQLite et racine de partage jetables, daemon Docker injoignable.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::header::{CONTENT_TYPE, COOKIE, LOCATION, RANGE, SET_COOKIE};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{Server, new_router};
use crate::config::Config;
use crate::store::Store;
use crate::{docker, files};

struct TestApp {
    router: Router,
    store: Arc<Store>,
    share: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn test_app() -> TestApp {
    let tmp = tempfile::tempdir().unwrap();
    let share = tmp.path().join("partage");
    std::fs::create_dir(&share).unwrap();
    std::fs::write(share.join("exemple.txt"), "0123456789").unwrap();

    let config = Config {
        listen_addr: ":0".into(),
        data_dir: tmp.path().join("data").to_string_lossy().into_owned(),
        share_roots: HashMap::from([("pool".into(), share.to_string_lossy().into_owned())]),
        session_cookie_secure: false,
        session_ttl_hours: 1,
        docker_host: "tcp://127.0.0.1:1".into(),
    };
    let store = Arc::new(Store::open(&config.data_dir).unwrap());
    let server = Arc::new(Server {
        docker: docker::Client::new(&config.docker_host).unwrap(),
        files: files::Manager::new(&config.share_roots).unwrap(),
        store: store.clone(),
        config,
    });
    TestApp {
        router: new_router(server),
        store,
        share,
        _tmp: tmp,
    }
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }

    /// Couple `nom=valeur` du cookie de session posé par la réponse.
    fn cookie(&self) -> String {
        let set = self.headers[SET_COOKIE].to_str().unwrap();
        set.split(';').next().unwrap().to_string()
    }
}

impl TestApp {
    async fn send(&self, req: Request<Body>) -> Reply {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            headers,
            body,
        }
    }

    async fn call(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<Value>,
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            req = req.header(COOKIE, c);
        }
        let body = match body {
            Some(v) => {
                req = req.header(CONTENT_TYPE, "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        self.send(req.body(body).unwrap()).await
    }

    /// Crée le compte administrateur initial et retourne son cookie.
    async fn setup_admin(&self) -> String {
        let r = self
            .call(
                Method::POST,
                "/api/v1/setup",
                None,
                Some(json!({"username": "admin", "password": "motdepasse-admin"})),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED);
        r.cookie()
    }

    async fn login(&self, username: &str, password: &str) -> Reply {
        self.call(
            Method::POST,
            "/api/v1/auth/login",
            None,
            Some(json!({"username": username, "password": password})),
        )
        .await
    }
}

#[tokio::test]
async fn test_setup_premier_compte_puis_refus_du_second() {
    let app = test_app();
    let status = app
        .call(Method::GET, "/api/v1/setup/status", None, None)
        .await;
    assert_eq!(status.json(), json!({"needsSetup": true}));

    let r = app
        .call(
            Method::POST,
            "/api/v1/setup",
            None,
            Some(json!({"username": "admin", "password": "motdepasse-admin"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(r.json()["role"], "admin");
    let set_cookie = r.headers[SET_COOKIE].to_str().unwrap();
    assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Strict"));

    let again = app
        .call(
            Method::POST,
            "/api/v1/setup",
            None,
            Some(json!({"username": "intrus", "password": "motdepasse-intrus"})),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    let status = app
        .call(Method::GET, "/api/v1/setup/status", None, None)
        .await;
    assert_eq!(status.json(), json!({"needsSetup": false}));
}

#[tokio::test]
async fn test_setup_champ_inconnu_retourne_400() {
    let app = test_app();
    let r = app
        .call(
            Method::POST,
            "/api/v1/setup",
            None,
            Some(json!({"username": "admin", "password": "motdepasse", "role": "admin"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json(), json!({"error": "requête invalide"}));
}

#[tokio::test]
async fn test_me_sans_et_avec_session() {
    let app = test_app();
    let cookie = app.setup_admin().await;

    let anon = app.call(Method::GET, "/api/v1/auth/me", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    let me = app
        .call(Method::GET, "/api/v1/auth/me", Some(&cookie), None)
        .await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["username"], "admin");
}

#[tokio::test]
async fn test_route_protegee_anonyme_401_utilisateur_simple_403() {
    let app = test_app();
    let admin = app.setup_admin().await;

    let anon = app.call(Method::GET, "/api/v1/users", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    assert_eq!(anon.headers[CONTENT_TYPE], "text/plain; charset=utf-8");
    assert_eq!(anon.json(), json!({"error": "authentification requise"}));

    let created = app
        .call(
            Method::POST,
            "/api/v1/users",
            Some(&admin),
            Some(json!({"username": "bob", "password": "motdepasse-bob", "role": "user"})),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);

    let bob = app.login("bob", "motdepasse-bob").await.cookie();
    let forbidden = app
        .call(Method::GET, "/api/v1/users", Some(&bob), None)
        .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    assert_eq!(
        forbidden.json(),
        json!({"error": "droits administrateur requis"})
    );

    let roots = app
        .call(Method::GET, "/api/v1/files/roots", Some(&bob), None)
        .await;
    assert_eq!(roots.status, StatusCode::OK);
    assert_eq!(roots.json(), json!(["pool"]));
}

#[tokio::test]
async fn test_login_mauvais_mot_de_passe_ou_compte_inconnu_401() {
    let app = test_app();
    app.setup_admin().await;
    assert_eq!(
        app.login("admin", "pas-le-bon").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("personne", "motdepasse-admin").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.login("admin", "motdepasse-admin").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_login_compte_cree_par_go_accepte() {
    let app = test_app();
    app.store
        .create_user(
            "vanti",
            "$2a$10$OOMwALbjxrKSVLbegTuj..SwleG/vNVYP0cSx3TZlcTgo9MpOcOg.",
            "admin",
        )
        .unwrap();
    assert_eq!(
        app.login("vanti", "mot-de-passe-correct").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_change_password_revoque_les_autres_sessions() {
    let app = test_app();
    let first = app.setup_admin().await;
    let other = app.login("admin", "motdepasse-admin").await.cookie();

    let wrong = app
        .call(
            Method::PATCH,
            "/api/v1/auth/password",
            Some(&first),
            Some(json!({"currentPassword": "faux", "newPassword": "nouveau-mdp-123"})),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);

    let changed = app
        .call(
            Method::PATCH,
            "/api/v1/auth/password",
            Some(&first),
            Some(json!({"currentPassword": "motdepasse-admin", "newPassword": "nouveau-mdp-123"})),
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);
    let renewed = changed.cookie();

    for (cookie, expected) in [
        (first.as_str(), StatusCode::UNAUTHORIZED),
        (other.as_str(), StatusCode::UNAUTHORIZED),
        (renewed.as_str(), StatusCode::OK),
    ] {
        let r = app
            .call(Method::GET, "/api/v1/auth/me", Some(cookie), None)
            .await;
        assert_eq!(r.status, expected, "{cookie}");
    }
    assert_eq!(
        app.login("admin", "nouveau-mdp-123").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn test_logout_invalide_la_session() {
    let app = test_app();
    let cookie = app.setup_admin().await;
    let r = app
        .call(Method::POST, "/api/v1/auth/logout", Some(&cookie), None)
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert!(
        r.headers[SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let me = app
        .call(Method::GET, "/api/v1/auth/me", Some(&cookie), None)
        .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_dernier_admin_ni_supprimable_ni_retrogradable() {
    let app = test_app();
    let cookie = app.setup_admin().await;
    let id = app.store.get_user_by_username("admin").unwrap().id;

    let del = app
        .call(
            Method::DELETE,
            &format!("/api/v1/users/{id}"),
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(del.status, StatusCode::CONFLICT);

    let demote = app
        .call(
            Method::PATCH,
            &format!("/api/v1/users/{id}"),
            Some(&cookie),
            Some(json!({"role": "user"})),
        )
        .await;
    assert_eq!(demote.status, StatusCode::CONFLICT);

    let unknown = app
        .call(Method::DELETE, "/api/v1/users/9999", Some(&cookie), None)
        .await;
    assert_eq!(unknown.status, StatusCode::CONFLICT);
    let bad_id = app
        .call(Method::DELETE, "/api/v1/users/abc", Some(&cookie), None)
        .await;
    assert_eq!(bad_id.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_reinitialisation_admin_revoque_les_sessions_du_compte() {
    let app = test_app();
    let admin = app.setup_admin().await;
    app.call(
        Method::POST,
        "/api/v1/users",
        Some(&admin),
        Some(json!({"username": "bob", "password": "motdepasse-bob", "role": "user"})),
    )
    .await;
    let bob = app.login("bob", "motdepasse-bob").await.cookie();
    let bob_id = app.store.get_user_by_username("bob").unwrap().id;

    let r = app
        .call(
            Method::PATCH,
            &format!("/api/v1/users/{bob_id}"),
            Some(&admin),
            Some(json!({"password": "nouveau-bob-123"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    let me = app
        .call(Method::GET, "/api/v1/auth/me", Some(&bob), None)
        .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_fichiers_upload_liste_telechargement_partiel() {
    let app = test_app();
    let cookie = app.setup_admin().await;

    let boundary = "frontiere";
    let multipart = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"n.txt\"\r\nContent-Type: text/plain\r\n\r\nbonjour le NAS\r\n--{boundary}--\r\n"
    );
    let req = Request::post("/api/v1/files/upload?root=pool&path=/envoye.txt")
        .header(COOKIE, &cookie)
        .header(
            CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(multipart))
        .unwrap();
    assert_eq!(app.send(req).await.status, StatusCode::CREATED);
    assert_eq!(
        std::fs::read_to_string(app.share.join("envoye.txt")).unwrap(),
        "bonjour le NAS"
    );

    let list = app
        .call(
            Method::GET,
            "/api/v1/files/list?root=pool&path=/",
            Some(&cookie),
            None,
        )
        .await;
    let names: Vec<_> = list
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].clone())
        .collect();
    assert_eq!(names, [json!("envoye.txt"), json!("exemple.txt")]);

    let req = Request::get("/api/v1/files/download?root=pool&path=/exemple.txt")
        .header(COOKIE, &cookie)
        .header(RANGE, "bytes=2-4")
        .body(Body::empty())
        .unwrap();
    let partial = app.send(req).await;
    assert_eq!(partial.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(&partial.body[..], b"234");
    assert_eq!(partial.headers["content-range"], "bytes 2-4/10");
    assert_eq!(
        partial.headers["content-disposition"],
        "attachment; filename=\"exemple.txt\""
    );
}

#[tokio::test]
async fn test_upload_sans_champ_file_retourne_400_sans_creer_de_fichier() {
    let app = test_app();
    let cookie = app.setup_admin().await;
    let req = Request::post("/api/v1/files/upload?root=pool&path=/vide.txt")
        .header(COOKIE, &cookie)
        .header(CONTENT_TYPE, "multipart/form-data; boundary=b")
        .body(Body::from(
            "--b\r\nContent-Disposition: form-data; name=\"autre\"\r\n\r\nx\r\n--b--\r\n",
        ))
        .unwrap();
    assert_eq!(app.send(req).await.status, StatusCode::BAD_REQUEST);
    assert!(!app.share.join("vide.txt").exists());
}

#[tokio::test]
async fn test_fichiers_racine_inconnue_404_traversee_refusee() {
    let app = test_app();
    let cookie = app.setup_admin().await;
    let unknown = app
        .call(
            Method::GET,
            "/api/v1/files/list?root=autre&path=/",
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    let escape = app
        .call(
            Method::GET,
            "/api/v1/files/download?root=pool&path=/../data/silo.db",
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(escape.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_spa_route_inconnue_sert_index_et_index_redirige() {
    let app = test_app();
    for path in ["/", "/docker", "/api/v1/inconnue", "/../Cargo.toml"] {
        let r = app.call(Method::GET, path, None, None).await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
        assert!(
            r.headers[CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html"),
            "{path}"
        );
    }
    let r = app.call(Method::GET, "/index.html", None, None).await;
    assert_eq!(r.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(r.headers[LOCATION], "./");
}

#[tokio::test]
async fn test_methode_non_prevue_retourne_405() {
    let app = test_app();
    let r = app
        .call(Method::DELETE, "/api/v1/setup/status", None, None)
        .await;
    assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_docker_injoignable_retourne_500_json() {
    let app = test_app();
    let cookie = app.setup_admin().await;
    let r = app
        .call(
            Method::GET,
            "/api/v1/docker/containers",
            Some(&cookie),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        r.json(),
        json!({"error": "impossible de lister les containers"})
    );
}

#[tokio::test]
async fn test_scrub_lecture_pour_tous_demarrage_reserve_admin() {
    let app = test_app();
    let admin = app.setup_admin().await;
    app.call(
        Method::POST,
        "/api/v1/users",
        Some(&admin),
        Some(json!({"username": "bob", "password": "motdepasse-bob", "role": "user"})),
    )
    .await;
    let bob = app.login("bob", "motdepasse-bob").await.cookie();

    // Le paramètre manquant prouve que la requête a atteint le handler.
    let read = app
        .call(Method::GET, "/api/v1/storage/scrub", Some(&bob), None)
        .await;
    assert_eq!(read.status, StatusCode::BAD_REQUEST);

    let start = app
        .call(
            Method::POST,
            "/api/v1/storage/scrub",
            Some(&bob),
            Some(json!({"path": "/mnt/pool"})),
        )
        .await;
    assert_eq!(start.status, StatusCode::FORBIDDEN);

    let anon = app
        .call(Method::POST, "/api/v1/storage/scrub", None, Some(json!({})))
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    let missing = app
        .call(
            Method::POST,
            "/api/v1/storage/scrub",
            Some(&admin),
            Some(json!({})),
        )
        .await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_dossier_retourne_400() {
    let app = test_app();
    let cookie = app.setup_admin().await;
    std::fs::create_dir(app.share.join("sous")).unwrap();
    for path in ["/sous", "/"] {
        let r = app
            .call(
                Method::GET,
                &format!("/api/v1/files/download?root=pool&path={path}"),
                Some(&cookie),
                None,
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(r.json(), json!({"error": "le chemin désigne un dossier"}));
    }
}
