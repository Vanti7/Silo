//! Boucle d'acceptation HTTP : délai de lecture des en-têtes (contre les
//! clients qui ouvrent des connexions sans jamais finir leur requête) et
//! fermeture propre bornée dans le temps.

use std::future::Future;
use std::time::Duration;

use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;

/// Délai maximal de réception des en-têtes d'une requête.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

// Pause après un échec d'accept (ex : limite de descripteurs atteinte),
// pour ne pas boucler à vide en saturant le CPU.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// Erreur de fermeture du serveur.
#[derive(Debug, thiserror::Error)]
#[error("fermeture propre non terminée après {}s", .0.as_secs())]
pub struct ShutdownTimeout(Duration);

/// Sert `app` jusqu'à ce que `shutdown` se résolve, puis laisse au plus
/// `grace` aux requêtes en cours pour se terminer.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    shutdown: impl Future<Output = ()>,
    grace: Duration,
) -> Result<(), ShutdownTimeout> {
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT);
    let graceful = GracefulShutdown::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let stream = match accepted {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        tracing::warn!(err = %e, "acceptation d'une connexion");
                        tokio::time::sleep(ACCEPT_ERROR_BACKOFF).await;
                        continue;
                    }
                };
                let service = TowerToHyperService::new(app.clone());
                let conn = builder
                    .serve_connection(TokioIo::new(stream), service)
                    .into_owned();
                let conn = graceful.watch(conn);
                tokio::spawn(async move {
                    // Une connexion interrompue par le client n'est pas une
                    // erreur du serveur.
                    let _ = conn.await;
                });
            }
            () = &mut shutdown => break,
        }
    }

    drop(listener);
    tokio::time::timeout(grace, graceful.shutdown())
        .await
        .map_err(|_| ShutdownTimeout(grace))
}

#[cfg(test)]
mod tests {
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn test_serve_repond_puis_s_arrete_sur_signal() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let app = Router::new().route("/", get(|| async { "ok" }));
        let server = tokio::spawn(serve(
            listener,
            app,
            async {
                let _ = rx.await;
            },
            Duration::from_secs(1),
        ));

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with("ok"));

        tx.send(()).unwrap();
        server.await.unwrap().unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn test_serve_entetes_incomplets_connexion_fermee_apres_delai() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/", get(|| async { "ok" }));
        tokio::spawn(serve(
            listener,
            app,
            std::future::pending(),
            Duration::from_secs(1),
        ));

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        client.write_all(b"GET / HTTP/1.1\r\nHost:").await.unwrap();
        let mut buf = Vec::new();
        let read = tokio::time::timeout(
            HEADER_READ_TIMEOUT + Duration::from_secs(5),
            client.read_to_end(&mut buf),
        )
        .await;
        assert!(
            read.is_ok(),
            "la connexion aurait dû être fermée par le serveur"
        );
    }
}
