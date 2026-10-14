//! Explorateur de fichiers confiné aux racines de partage configurées.
//! Chaque racine est ouverte comme capacité `cap_std::fs::Dir` : toute
//! résolution de chemin se fait relativement à ce descripteur, et le
//! système refuse d'en sortir, y compris via `..` ou un lien symbolique.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read};
use std::time::SystemTime;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use serde::Serialize;
use time::OffsetDateTime;
use time::macros::format_description;

/// Erreur de l'explorateur de fichiers.
#[derive(Debug, thiserror::Error)]
pub enum FilesError {
    #[error("files: racine de partage inconnue")]
    UnknownRoot,
    #[error("files: ouverture racine {name:?} ({path}): {source}")]
    OpenRoot {
        name: String,
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("files: {action} {path:?}: {source}")]
    Io {
        action: &'static str,
        path: String,
        #[source]
        source: io::Error,
    },
}

fn io_err(action: &'static str, path: &str) -> impl FnOnce(io::Error) -> FilesError {
    let path = path.to_string();
    move |source| FilesError::Io {
        action,
        path,
        source,
    }
}

/// Fichier ou dossier listé.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub mod_time: String,
}

/// Fichier ouvert en lecture, avec ses métadonnées.
pub struct OpenedFile {
    pub file: std::fs::File,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Accès aux racines de partage configurées.
pub struct Manager {
    roots: BTreeMap<String, Dir>,
}

impl Manager {
    /// Ouvre chaque racine (nom -> chemin absolu sur l'hôte).
    pub fn new(roots: &HashMap<String, String>) -> Result<Self, FilesError> {
        let mut opened = BTreeMap::new();
        for (name, path) in roots {
            let dir = Dir::open_ambient_dir(path, ambient_authority()).map_err(|source| {
                FilesError::OpenRoot {
                    name: name.clone(),
                    path: path.clone(),
                    source,
                }
            })?;
            opened.insert(name.clone(), dir);
        }
        Ok(Self { roots: opened })
    }

    /// Noms des racines configurées, triés.
    pub fn roots(&self) -> Vec<String> {
        self.roots.keys().cloned().collect()
    }

    fn root(&self, name: &str) -> Result<&Dir, FilesError> {
        self.roots.get(name).ok_or(FilesError::UnknownRoot)
    }

    /// Contenu d'un dossier : dossiers d'abord, puis tri par nom.
    pub fn list(&self, root_name: &str, dir: &str) -> Result<Vec<Entry>, FilesError> {
        let root = self.root(root_name)?;
        let read_dir = root
            .read_dir(to_fs_path(dir))
            .map_err(io_err("lecture dossier", dir))?;

        let mut out: Vec<Entry> = read_dir
            .filter_map(Result::ok)
            .filter_map(|entry| {
                // Métadonnées sans suivre les liens : un lien vers un dossier
                // est listé comme un fichier, comme avant.
                let meta = entry.metadata().ok()?;
                Some(Entry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    is_dir: meta.is_dir(),
                    size_bytes: meta.len(),
                    mod_time: meta
                        .modified()
                        .map(|t| format_mod_time(t.into_std()))
                        .unwrap_or_default(),
                })
            })
            .collect();
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(out)
    }

    /// Ouvre un fichier en lecture pour téléchargement.
    pub fn open(&self, root_name: &str, file_path: &str) -> Result<OpenedFile, FilesError> {
        let root = self.root(root_name)?;
        let file = root
            .open(to_fs_path(file_path))
            .map_err(io_err("ouverture fichier", file_path))?;
        let meta = file.metadata().map_err(io_err("stat fichier", file_path))?;
        Ok(OpenedFile {
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified: meta.modified().ok().map(|t| t.into_std()),
            file: file.into_std(),
        })
    }

    /// Crée ou remplace un fichier avec le contenu lu depuis `src`.
    pub fn write_file(
        &self,
        root_name: &str,
        file_path: &str,
        src: &mut impl Read,
    ) -> Result<(), FilesError> {
        let root = self.root(root_name)?;
        let mut file = root
            .create(to_fs_path(file_path))
            .map_err(io_err("création fichier", file_path))?;
        io::copy(src, &mut file).map_err(io_err("écriture fichier", file_path))?;
        Ok(())
    }

    /// Crée un dossier et ses parents éventuels.
    pub fn mkdir(&self, root_name: &str, dir_path: &str) -> Result<(), FilesError> {
        self.root(root_name)?
            .create_dir_all(to_fs_path(dir_path))
            .map_err(io_err("création dossier", dir_path))
    }

    /// Supprime un fichier ou un dossier (récursivement). Une cible absente
    /// n'est pas une erreur ; la racine elle-même ne peut pas être supprimée.
    pub fn remove(&self, root_name: &str, target_path: &str) -> Result<(), FilesError> {
        let root = self.root(root_name)?;
        let fs_path = to_fs_path(target_path);
        let err = io_err("suppression", target_path);
        if fs_path == "." {
            return Err(err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "la racine de partage ne peut pas être supprimée",
            )));
        }
        let result = match root.symlink_metadata(&fs_path) {
            Ok(meta) if meta.is_dir() => root.remove_dir_all(&fs_path),
            Ok(_) => root.remove_file(&fs_path),
            Err(e) => Err(e),
        };
        match result {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other.map_err(err),
        }
    }

    /// Renomme ou déplace une entrée au sein d'une même racine.
    pub fn rename(
        &self,
        root_name: &str,
        old_path: &str,
        new_path: &str,
    ) -> Result<(), FilesError> {
        let root = self.root(root_name)?;
        root.rename(to_fs_path(old_path), root, to_fs_path(new_path))
            .map_err(|source| FilesError::Io {
                action: "renommage",
                path: format!("{old_path} -> {new_path}"),
                source,
            })
    }
}

/// Normalise un chemin fourni par le client (`/`, vide, avec des `..`…) en
/// chemin relatif à la racine : `.` pour la racine, jamais de `/` en tête,
/// les `..` étant neutralisés lexicalement avant tout accès disque.
fn to_fs_path(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

fn format_mod_time(t: SystemTime) -> String {
    OffsetDateTime::from(t)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
        ))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_to_fs_path_neutralise_les_remontees() {
        let cases = [
            ("", "."),
            ("/", "."),
            ("/foo", "foo"),
            ("foo/bar", "foo/bar"),
            ("/foo/../../..", "."),
            ("/../../etc", "etc"),
            ("/a/../b", "b"),
            ("//a//./b/", "a/b"),
        ];
        for (input, want) in cases {
            assert_eq!(to_fs_path(input), want, "{input:?}");
        }
    }

    fn test_manager() -> (Manager, tempfile::TempDir) {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("racine");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("exemple.txt"), "contenu").unwrap();
        fs::create_dir(dir.join("sous")).unwrap();
        let roots = HashMap::from([("test".to_string(), dir.to_string_lossy().into_owned())]);
        (Manager::new(&roots).unwrap(), parent)
    }

    #[test]
    fn test_list_dossiers_avant_fichiers() {
        let (m, _tmp) = test_manager();
        let entries = m.list("test", "/").unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "sous");
        assert_eq!(entries[1].name, "exemple.txt");
        assert_eq!(entries[1].size_bytes, 7);
        assert!(entries[1].mod_time.ends_with('Z'));
    }

    #[test]
    fn test_list_racine_inconnue_retourne_unknown_root() {
        let (m, _tmp) = test_manager();
        assert!(matches!(
            m.list("inconnue", "/"),
            Err(FilesError::UnknownRoot)
        ));
    }

    #[test]
    fn test_open_traversee_de_chemin_reste_dans_la_racine() {
        let (m, tmp) = test_manager();
        fs::write(tmp.path().join("secret.txt"), "hors racine").unwrap();
        assert!(m.open("test", "/../../secret.txt").is_err());
        assert!(m.open("test", "../secret.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_open_lien_symbolique_sortant_refuse() {
        let (m, tmp) = test_manager();
        fs::write(tmp.path().join("secret.txt"), "hors racine").unwrap();
        std::os::unix::fs::symlink(
            tmp.path().join("secret.txt"),
            tmp.path().join("racine/lien"),
        )
        .unwrap();
        assert!(m.open("test", "/lien").is_err());
    }

    #[test]
    fn test_write_file_puis_open_taille_identique() {
        let (m, _tmp) = test_manager();
        m.write_file("test", "/nouveau.txt", &mut "bonjour".as_bytes())
            .unwrap();
        let f = m.open("test", "/nouveau.txt").unwrap();
        assert_eq!(f.size, 7);
        assert!(!f.is_dir);
    }

    #[test]
    fn test_mkdir_puis_remove_recursif() {
        let (m, _tmp) = test_manager();
        m.mkdir("test", "/a/b/c").unwrap();
        let entries = m.list("test", "/a/b").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "c");

        m.remove("test", "/a").unwrap();
        assert!(m.list("test", "/a").is_err());
    }

    #[test]
    fn test_remove_absent_ne_retourne_pas_d_erreur() {
        let (m, _tmp) = test_manager();
        m.remove("test", "/inexistant").unwrap();
    }

    #[test]
    fn test_remove_racine_refuse_et_contenu_intact() {
        let (m, _tmp) = test_manager();
        assert!(m.remove("test", "/").is_err());
        assert!(m.remove("test", "/sous/..").is_err());
        assert_eq!(m.list("test", "/").unwrap().len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn test_list_liens_symboliques_listes_comme_fichiers() {
        let (m, tmp) = test_manager();
        let racine = tmp.path().join("racine");
        std::os::unix::fs::symlink(racine.join("sous"), racine.join("lien-dossier")).unwrap();
        std::os::unix::fs::symlink(tmp.path(), racine.join("lien-sortant")).unwrap();
        let entries = m.list("test", "/").unwrap();
        for name in ["lien-dossier", "lien-sortant"] {
            let e = entries.iter().find(|e| e.name == name).unwrap();
            assert!(!e.is_dir, "{name}");
        }
    }

    #[test]
    fn test_rename_ancien_nom_disparait() {
        let (m, _tmp) = test_manager();
        m.rename("test", "/exemple.txt", "/renomme.txt").unwrap();
        assert!(m.open("test", "/renomme.txt").is_ok());
        assert!(m.open("test", "/exemple.txt").is_err());
    }

    #[test]
    fn test_roots_triees() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().to_string_lossy().into_owned();
        let roots = HashMap::from([("zeta".to_string(), p.clone()), ("alpha".to_string(), p)]);
        assert_eq!(Manager::new(&roots).unwrap().roots(), ["alpha", "zeta"]);
    }
}
