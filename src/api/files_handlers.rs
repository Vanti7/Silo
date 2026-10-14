use std::io::{Seek, SeekFrom};

use axum::body::Bytes;
use axum::extract::multipart::{Multipart, MultipartRejection};
use axum::extract::{Query, State};
use axum::http::header::CONTENT_DISPOSITION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use super::download::{path_escape, serve_file};
use super::{AppState, blocking, decode_json, write_error, write_json};
use crate::files::FilesError;

/// Taille maximale d'un envoi : limite raisonnable pour un explorateur web,
/// qui évite qu'un envoi non borné épuise la mémoire ou le disque du NAS.
pub(super) const UPLOAD_LIMIT: usize = 1 << 30;

#[derive(Deserialize)]
pub(super) struct FileQuery {
    #[serde(default)]
    root: String,
    #[serde(default)]
    path: String,
}

fn missing_root_and_path() -> Response {
    write_error(
        StatusCode::BAD_REQUEST,
        "paramètres 'root' et 'path' requis",
    )
}

/// Noms des racines de partage configurées.
pub(super) async fn list_roots(State(s): State<AppState>) -> Response {
    write_json(StatusCode::OK, &s.files.roots())
}

/// Contenu d'un dossier (`?root=nom&path=/sous/dossier`).
pub(super) async fn list_dir(State(s): State<AppState>, Query(q): Query<FileQuery>) -> Response {
    if q.root.is_empty() {
        return write_error(StatusCode::BAD_REQUEST, "paramètre 'root' requis");
    }
    match blocking(move || s.files.list(&q.root, &q.path)).await {
        Ok(entries) => write_json(StatusCode::OK, &entries),
        Err(e) => write_files_err(&e),
    }
}

/// Diffuse un fichier en téléchargement.
pub(super) async fn download_file(
    State(s): State<AppState>,
    Query(q): Query<FileQuery>,
    headers: HeaderMap,
) -> Response {
    if q.root.is_empty() || q.path.is_empty() {
        return missing_root_and_path();
    }
    let file = match s.files.open(&q.root, &q.path) {
        Ok(f) => f,
        Err(e) => return write_files_err(&e),
    };
    if file.is_dir {
        return write_error(StatusCode::BAD_REQUEST, "le chemin désigne un dossier");
    }

    let filename = base_name(&q.path);
    let disposition = format!("attachment; filename=\"{}\"", path_escape(filename));
    (
        [(CONTENT_DISPOSITION, disposition)],
        serve_file(&headers, filename, file).await,
    )
        .into_response()
}

/// Dernier élément d'un chemin, comme `path.Base` en Go.
fn base_name(p: &str) -> &str {
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() {
        return if p.is_empty() { "." } else { "/" };
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// Reçoit un fichier en multipart/form-data (champ `file`) et l'écrit à
/// `root:path`.
pub(super) async fn upload_file(
    State(s): State<AppState>,
    Query(q): Query<FileQuery>,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    if q.root.is_empty() || q.path.is_empty() {
        return missing_root_and_path();
    }
    let received = match multipart {
        Ok(mut m) => receive_file_field(&mut m).await,
        Err(_) => Ok(None),
    };
    let Ok(Some(mut tmp)) = received else {
        return write_error(
            StatusCode::BAD_REQUEST,
            "champ 'file' manquant ou requête trop volumineuse",
        );
    };
    match blocking(move || s.files.write_file(&q.root, &q.path, &mut tmp)).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => write_files_err(&e),
    }
}

/// Recopie le champ `file` dans un fichier temporaire anonyme. Le fichier
/// de destination n'est créé qu'une fois l'envoi entièrement reçu : un envoi
/// interrompu ou trop volumineux ne laisse pas de fichier tronqué.
async fn receive_file_field(m: &mut Multipart) -> anyhow::Result<Option<std::fs::File>> {
    while let Some(mut field) = m.next_field().await? {
        if field.name() != Some("file") {
            continue;
        }
        let mut out = tokio::fs::File::from_std(tempfile::tempfile()?);
        while let Some(chunk) = field.chunk().await? {
            out.write_all(&chunk).await?;
        }
        out.flush().await?;
        let mut file = out.into_std().await;
        file.seek(SeekFrom::Start(0))?;
        return Ok(Some(file));
    }
    Ok(None)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MkdirRequest {
    #[serde(default)]
    root: String,
    #[serde(default)]
    path: String,
}

/// Crée un dossier depuis un corps JSON `{"root", "path"}`.
pub(super) async fn mkdir(State(s): State<AppState>, body: Bytes) -> Response {
    let req = match decode_json::<MkdirRequest>(&body) {
        Ok(r) if !r.root.is_empty() && !r.path.is_empty() => r,
        _ => return missing_root_and_path(),
    };
    match blocking(move || s.files.mkdir(&req.root, &req.path)).await {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => write_files_err(&e),
    }
}

/// Supprime un fichier ou dossier (`?root=nom&path=/chemin`).
pub(super) async fn delete_file(State(s): State<AppState>, Query(q): Query<FileQuery>) -> Response {
    if q.root.is_empty() || q.path.is_empty() {
        return missing_root_and_path();
    }
    match blocking(move || s.files.remove(&q.root, &q.path)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => write_files_err(&e),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RenameRequest {
    #[serde(default)]
    root: String,
    #[serde(default)]
    old_path: String,
    #[serde(default)]
    new_path: String,
}

/// Renomme ou déplace une entrée (`{"root", "oldPath", "newPath"}`).
pub(super) async fn rename_file(State(s): State<AppState>, body: Bytes) -> Response {
    let req = match decode_json::<RenameRequest>(&body) {
        Ok(r) if !r.root.is_empty() && !r.old_path.is_empty() && !r.new_path.is_empty() => r,
        _ => {
            return write_error(
                StatusCode::BAD_REQUEST,
                "paramètres 'root', 'oldPath' et 'newPath' requis",
            );
        }
    };
    match blocking(move || s.files.rename(&req.root, &req.old_path, &req.new_path)).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => write_files_err(&e),
    }
}

fn write_files_err(e: &FilesError) -> Response {
    match e {
        FilesError::UnknownRoot => write_error(StatusCode::NOT_FOUND, "racine de partage inconnue"),
        _ => write_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_name_comme_path_base_go() {
        assert_eq!(base_name("/a/b/c.txt"), "c.txt");
        assert_eq!(base_name("c.txt"), "c.txt");
        assert_eq!(base_name("/a/b/"), "b");
        assert_eq!(base_name("/"), "/");
    }
}
