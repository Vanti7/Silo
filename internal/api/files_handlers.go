package api

import (
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"path"

	"silo/internal/files"
)

// handleListRoots retourne les noms des racines de partage configurées.
func (s *Server) handleListRoots(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, s.Files.Roots())
}

// fileParams extrait et valide les paramètres 'root' et 'path' communs à
// tous les endpoints de l'explorateur de fichiers.
func fileParams(r *http.Request) (root, p string) {
	q := r.URL.Query()
	return q.Get("root"), q.Get("path")
}

// handleListDir liste le contenu d'un dossier (?root=nom&path=/sous/dossier).
func (s *Server) handleListDir(w http.ResponseWriter, r *http.Request) {
	root, p := fileParams(r)
	if root == "" {
		writeError(w, http.StatusBadRequest, "paramètre 'root' requis")
		return
	}
	entries, err := s.Files.List(root, p)
	if err != nil {
		writeFilesErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, entries)
}

// handleDownloadFile diffuse un fichier en téléchargement.
func (s *Server) handleDownloadFile(w http.ResponseWriter, r *http.Request) {
	root, p := fileParams(r)
	if root == "" || p == "" {
		writeError(w, http.StatusBadRequest, "paramètres 'root' et 'path' requis")
		return
	}
	f, info, err := s.Files.Open(root, p)
	if err != nil {
		writeFilesErr(w, err)
		return
	}
	defer f.Close()

	if info.IsDir() {
		writeError(w, http.StatusBadRequest, "le chemin désigne un dossier")
		return
	}

	filename := path.Base(p)
	w.Header().Set("Content-Disposition", fmt.Sprintf(`attachment; filename="%s"`, url.PathEscape(filename)))
	http.ServeContent(w, r, filename, info.ModTime(), f)
}

// handleUploadFile reçoit un fichier envoyé en multipart/form-data (champ
// "file") et l'écrit à root:path.
func (s *Server) handleUploadFile(w http.ResponseWriter, r *http.Request) {
	root, p := fileParams(r)
	if root == "" || p == "" {
		writeError(w, http.StatusBadRequest, "paramètres 'root' et 'path' requis")
		return
	}

	// 1 GiB max par envoi : limite raisonnable pour un explorateur web,
	// évite qu'un envoi non borné n'épuise la mémoire ou le disque du NAS.
	r.Body = http.MaxBytesReader(w, r.Body, 1<<30)
	file, _, err := r.FormFile("file")
	if err != nil {
		writeError(w, http.StatusBadRequest, "champ 'file' manquant ou requête trop volumineuse")
		return
	}
	defer file.Close()

	if err := s.Files.WriteFile(root, p, file); err != nil {
		writeFilesErr(w, err)
		return
	}
	w.WriteHeader(http.StatusCreated)
}

// handleMkdir crée un dossier depuis un corps JSON {"root", "path"}.
func (s *Server) handleMkdir(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Root string `json:"root"`
		Path string `json:"path"`
	}
	if err := decodeJSON(r, &req); err != nil || req.Root == "" || req.Path == "" {
		writeError(w, http.StatusBadRequest, "paramètres 'root' et 'path' requis")
		return
	}
	if err := s.Files.Mkdir(req.Root, req.Path); err != nil {
		writeFilesErr(w, err)
		return
	}
	w.WriteHeader(http.StatusCreated)
}

// handleDeleteFile supprime un fichier ou dossier (?root=nom&path=/chemin).
func (s *Server) handleDeleteFile(w http.ResponseWriter, r *http.Request) {
	root, p := fileParams(r)
	if root == "" || p == "" {
		writeError(w, http.StatusBadRequest, "paramètres 'root' et 'path' requis")
		return
	}
	if err := s.Files.Remove(root, p); err != nil {
		writeFilesErr(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// handleRenameFile renomme/déplace une entrée depuis un corps JSON
// {"root", "oldPath", "newPath"}.
func (s *Server) handleRenameFile(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Root    string `json:"root"`
		OldPath string `json:"oldPath"`
		NewPath string `json:"newPath"`
	}
	if err := decodeJSON(r, &req); err != nil || req.Root == "" || req.OldPath == "" || req.NewPath == "" {
		writeError(w, http.StatusBadRequest, "paramètres 'root', 'oldPath' et 'newPath' requis")
		return
	}
	if err := s.Files.Rename(req.Root, req.OldPath, req.NewPath); err != nil {
		writeFilesErr(w, err)
		return
	}
	w.WriteHeader(http.StatusOK)
}

func writeFilesErr(w http.ResponseWriter, err error) {
	if errors.Is(err, files.ErrUnknownRoot) {
		writeError(w, http.StatusNotFound, "racine de partage inconnue")
		return
	}
	if errors.Is(err, io.EOF) {
		writeError(w, http.StatusBadRequest, "requête invalide")
		return
	}
	writeError(w, http.StatusInternalServerError, err.Error())
}
