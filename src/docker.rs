//! Client Docker (API Engine via bollard) limité aux opérations utiles à
//! l'interface : containers, logs, images, état du daemon.

use std::collections::HashMap;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters::{
    ListContainersOptionsBuilder, ListImagesOptions, LogsOptionsBuilder,
    RemoveContainerOptionsBuilder,
};
pub use log_stream::LogStream;
use serde::Serialize;
use tokio::sync::OnceCell;

/// Erreur d'un appel au daemon Docker.
#[derive(Debug, thiserror::Error)]
#[error("docker: {context}: {source}")]
pub struct DockerError {
    context: String,
    #[source]
    source: bollard::errors::Error,
}

fn docker_err(context: impl Into<String>) -> impl FnOnce(bollard::errors::Error) -> DockerError {
    let context = context.into();
    move |source| DockerError { context, source }
}

/// Client Docker partagé par les handlers.
pub struct Client {
    host: String,
    // Connexion établie et version d'API négociée au premier appel abouti,
    // et non au démarrage : silo doit démarrer même si le daemon est absent
    // ou pas encore prêt.
    connected: OnceCell<Docker>,
}

fn connect(host: &str) -> Result<Docker, bollard::errors::Error> {
    if host.is_empty() {
        Docker::connect_with_defaults()
    } else {
        Docker::connect_with_host(host)
    }
}

impl Client {
    /// Crée un client. `host` vide utilise `DOCKER_HOST` ou, à défaut, le
    /// socket local. Seul un hôte mal formé est refusé ici ; un socket
    /// absent n'est signalé qu'à l'usage.
    pub fn new(host: &str) -> Result<Self, DockerError> {
        match connect(host) {
            Ok(_) | Err(bollard::errors::Error::SocketNotFoundError(_)) => Ok(Self {
                host: host.to_string(),
                connected: OnceCell::new(),
            }),
            Err(e) => Err(docker_err("connexion au daemon")(e)),
        }
    }

    async fn cli(&self) -> Result<&Docker, DockerError> {
        self.connected
            .get_or_try_init(|| async { connect(&self.host)?.negotiate_version().await })
            .await
            .map_err(docker_err("connexion au daemon"))
    }

    /// Tous les containers, démarrés ou non.
    pub async fn list_containers(&self) -> Result<Vec<Container>, DockerError> {
        let opts = ListContainersOptionsBuilder::new().all(true).build();
        let list = self
            .cli()
            .await?
            .list_containers(Some(opts))
            .await
            .map_err(docker_err("liste containers"))?;
        Ok(list
            .into_iter()
            .map(|c| Container {
                id: c.id.unwrap_or_default(),
                names: c.names.unwrap_or_default(),
                image: c.image.unwrap_or_default(),
                state: c.state.map(|s| s.to_string()).unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                ports: c
                    .ports
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| Port {
                        private_port: p.private_port,
                        public_port: p.public_port.unwrap_or(0),
                        typ: p.typ.map(|t| t.to_string()).unwrap_or_default(),
                    })
                    .collect(),
                created: c.created.unwrap_or(0),
                labels: c.labels.unwrap_or_default(),
            })
            .collect())
    }

    /// Démarre un container.
    pub async fn start_container(&self, id: &str) -> Result<(), DockerError> {
        self.cli()
            .await?
            .start_container(id, None)
            .await
            .map_err(docker_err(format!("démarrage container {id}")))
    }

    /// Arrête un container (délai de grâce géré par le daemon).
    pub async fn stop_container(&self, id: &str) -> Result<(), DockerError> {
        self.cli()
            .await?
            .stop_container(id, None)
            .await
            .map_err(docker_err(format!("arrêt container {id}")))
    }

    /// Redémarre un container.
    pub async fn restart_container(&self, id: &str) -> Result<(), DockerError> {
        self.cli()
            .await?
            .restart_container(id, None)
            .await
            .map_err(docker_err(format!("redémarrage container {id}")))
    }

    /// Supprime un container ; il doit être arrêté, sauf si `force`.
    pub async fn remove_container(&self, id: &str, force: bool) -> Result<(), DockerError> {
        let opts = RemoveContainerOptionsBuilder::new().force(force).build();
        self.cli()
            .await?
            .remove_container(id, Some(opts))
            .await
            .map_err(docker_err(format!("suppression container {id}")))
    }

    /// Flux des logs (stdout + stderr, horodatés) d'un container. Le flux
    /// est déjà démultiplexé : seul le texte des logs est restitué.
    pub async fn container_logs(
        &self,
        id: &str,
        follow: bool,
        tail: &str,
    ) -> Result<LogStream, DockerError> {
        let opts = LogsOptionsBuilder::new()
            .stdout(true)
            .stderr(true)
            .follow(follow)
            .tail(tail)
            .timestamps(true)
            .build();
        Ok(LogStream::new(
            self.cli().await?.logs(id, Some(opts)),
            format!("logs container {id}"),
        ))
    }

    /// Images présentes localement.
    pub async fn list_images(&self) -> Result<Vec<Image>, DockerError> {
        let list = self
            .cli()
            .await?
            .list_images(None::<ListImagesOptions>)
            .await
            .map_err(docker_err("liste images"))?;
        Ok(list
            .into_iter()
            .map(|i| Image {
                id: i.id,
                tags: i.repo_tags,
                size_bytes: i.size,
                created: i.created,
            })
            .collect())
    }

    /// Résumé de l'état du daemon.
    pub async fn get_info(&self) -> Result<Info, DockerError> {
        let info = self
            .cli()
            .await?
            .info()
            .await
            .map_err(docker_err("info daemon"))?;
        Ok(Info {
            server_version: info.server_version.unwrap_or_default(),
            containers_running: info.containers_running.unwrap_or(0),
            containers_stopped: info.containers_stopped.unwrap_or(0),
            images: info.images.unwrap_or(0),
            storage_driver: info.driver.unwrap_or_default(),
        })
    }
}

/// Container résumé pour l'affichage en liste.
#[derive(Debug, Serialize)]
pub struct Container {
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: Vec<Port>,
    pub created: i64,
    pub labels: HashMap<String, String>,
}

/// Port publié par un container.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub private_port: u16,
    pub public_port: u16,
    #[serde(rename = "type")]
    pub typ: String,
}

/// Image Docker locale.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub id: String,
    pub tags: Vec<String>,
    pub size_bytes: i64,
    pub created: i64,
}

/// Résumé de l'état du daemon Docker.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub server_version: String,
    pub containers_running: i64,
    pub containers_stopped: i64,
    pub images: i64,
    pub storage_driver: String,
}

mod log_stream {
    use std::pin::Pin;

    use futures_util::{Stream, StreamExt};

    use super::{DockerError, LogOutput, docker_err};

    type Inner = Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>;

    /// Flux de logs d'un container, réduit au texte brut de chaque trame.
    pub struct LogStream {
        inner: Inner,
        context: String,
    }

    impl LogStream {
        pub(super) fn new(
            inner: impl Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send + 'static,
            context: String,
        ) -> Self {
            Self {
                inner: Box::pin(inner),
                context,
            }
        }

        /// Trame suivante ; `None` en fin de flux.
        pub async fn next_chunk(&mut self) -> Option<Result<bytes::Bytes, DockerError>> {
            let item = self.inner.next().await?;
            Some(
                item.map(LogOutput::into_bytes)
                    .map_err(docker_err(self.context.clone())),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn test_new_socket_absent_accepte_puis_erreur_a_l_usage() {
        let client = Client::new("unix:///chemin/inexistant/docker.sock").unwrap();
        assert!(client.get_info().await.is_err());
    }

    #[test]
    fn test_new_hote_mal_forme_refuse() {
        assert!(Client::new("ftp://nas:21").is_err());
    }
}
