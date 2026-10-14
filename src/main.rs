//! Démarre le serveur HTTP de l'interface d'administration du NAS (API +
//! frontend embarqué), ou exécute une sous-commande d'administration.

mod reset_password;

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use silo::api::{self, Server};
use silo::config::Config;
use silo::serve::serve;
use silo::store::Store;
use silo::{docker, files};
use tokio::net::TcpListener;
use tracing::{error, info, warn};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_PURGE_INTERVAL: Duration = Duration::from_secs(3600);

const USAGE: &str = "silo - interface d'administration du NAS

Usage :
  silo                              démarre le serveur HTTP
  silo reset-password <utilisateur> réinitialise le mot de passe d'un compte

La configuration est lue depuis les variables d'environnement (voir README).
";

fn main() -> ExitCode {
    // Sans sous-commande, silo démarre le serveur : c'est le mode utilisé
    // par l'unité systemd.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(name) = args.first() {
        return match run_subcommand(name, &args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("erreur : {e}");
                ExitCode::FAILURE
            }
        };
    }

    // Sous systemd, la sortie part dans journald : pas de codes couleur.
    tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(std::io::stdout().is_terminal())
        .init();
    let result = tokio::runtime::Runtime::new()
        .context("démarrage du runtime tokio")
        .and_then(|rt| rt.block_on(run()));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(err = %e, "arrêt sur erreur fatale");
            ExitCode::FAILURE
        }
    }
}

fn run_subcommand(name: &str, args: &[String]) -> anyhow::Result<()> {
    match name {
        "reset-password" => reset_password::run(args),
        "-h" | "--help" | "help" => {
            eprint!("{USAGE}");
            Ok(())
        }
        _ => {
            eprint!("{USAGE}");
            bail!("sous-commande inconnue : {name:?}")
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let config = Config::load()?;
    let store = Arc::new(Store::open(&config.data_dir)?);
    let docker = docker::Client::new(&config.docker_host)?;
    let files = files::Manager::new(&config.share_roots)?;
    let listener = bind(&config.listen_addr).await?;

    tokio::spawn(expire_sessions_periodically(store.clone()));

    info!(addr = %config.listen_addr, "démarrage du serveur");
    let app = api::new_router(Arc::new(Server {
        config,
        store,
        docker,
        files,
    }));

    let shutdown = async {
        shutdown_signal().await;
        info!("arrêt demandé, fermeture propre du serveur");
    };
    serve(listener, app, shutdown, SHUTDOWN_TIMEOUT).await?;
    Ok(())
}

/// Adresses à essayer pour une adresse d'écoute au format Go : `:8080`
/// signifie toutes les interfaces, en double pile IPv6/IPv4 si l'hôte
/// l'autorise, IPv4 seule sinon.
fn bind_candidates(addr: &str) -> Vec<String> {
    match addr.strip_prefix(':') {
        Some(port) => vec![format!("[::]:{port}"), format!("0.0.0.0:{port}")],
        None => vec![addr.to_string()],
    }
}

async fn bind(addr: &str) -> anyhow::Result<TcpListener> {
    let mut last_err = None;
    for candidate in bind_candidates(addr) {
        match TcpListener::bind(&candidate).await {
            Ok(listener) => return Ok(listener),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("au moins une adresse candidate"))
        .with_context(|| format!("écoute sur {addr} (SILO_LISTEN_ADDR)"))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

/// Purge horaire des sessions expirées, pour borner la table sessions.
async fn expire_sessions_periodically(store: Arc<Store>) {
    let mut ticker = tokio::time::interval_at(
        tokio::time::Instant::now() + SESSION_PURGE_INTERVAL,
        SESSION_PURGE_INTERVAL,
    );
    loop {
        ticker.tick().await;
        if let Err(e) = store.delete_expired_sessions() {
            warn!(err = %e, "purge des sessions expirées");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_candidates_port_seul_double_pile_puis_ipv4() {
        assert_eq!(bind_candidates(":8080"), ["[::]:8080", "0.0.0.0:8080"]);
    }

    #[test]
    fn test_bind_candidates_adresse_complete_inchangee() {
        assert_eq!(bind_candidates("127.0.0.1:8080"), ["127.0.0.1:8080"]);
        assert_eq!(bind_candidates("[::1]:9000"), ["[::1]:9000"]);
    }

    #[test]
    fn test_run_subcommand_inconnue_retourne_erreur() {
        assert!(run_subcommand("inconnue", &[]).is_err());
        assert!(run_subcommand("--help", &[]).is_ok());
    }
}
