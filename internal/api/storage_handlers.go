package api

import (
	"context"
	"net/http"

	"silo/internal/btrfs"
)

// handleListFilesystems retourne les systèmes de fichiers btrfs détectés.
func (s *Server) handleListFilesystems(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), btrfs.DefaultTimeout)
	defer cancel()

	filesystems, err := btrfs.ListFilesystems(ctx)
	if err != nil {
		s.Logger.Error("liste systèmes de fichiers btrfs", "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lire les systèmes de fichiers btrfs")
		return
	}
	writeJSON(w, http.StatusOK, filesystems)
}

// mountPointParam extrait le point de montage requis en query string,
// commun aux endpoints agissant sur un système de fichiers btrfs précis.
func mountPointParam(w http.ResponseWriter, r *http.Request) (string, bool) {
	mp := r.URL.Query().Get("path")
	if mp == "" {
		writeError(w, http.StatusBadRequest, "paramètre 'path' (point de montage) requis")
		return "", false
	}
	return mp, true
}

// handleFilesystemUsage retourne la répartition d'allocation d'un système
// de fichiers btrfs identifié par son point de montage (?path=/mnt/pool).
func (s *Server) handleFilesystemUsage(w http.ResponseWriter, r *http.Request) {
	mp, ok := mountPointParam(w, r)
	if !ok {
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), btrfs.DefaultTimeout)
	defer cancel()

	usage, err := btrfs.GetUsage(ctx, mp)
	if err != nil {
		s.Logger.Error("usage btrfs", "path", mp, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lire l'usage du système de fichiers")
		return
	}
	writeJSON(w, http.StatusOK, usage)
}

// handleListSubvolumes retourne les sous-volumes d'un système de fichiers
// btrfs identifié par son point de montage (?path=/mnt/pool).
func (s *Server) handleListSubvolumes(w http.ResponseWriter, r *http.Request) {
	mp, ok := mountPointParam(w, r)
	if !ok {
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), btrfs.DefaultTimeout)
	defer cancel()

	subvols, err := btrfs.ListSubvolumes(ctx, mp)
	if err != nil {
		s.Logger.Error("liste sous-volumes btrfs", "path", mp, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lire les sous-volumes")
		return
	}
	writeJSON(w, http.StatusOK, subvols)
}

// handleStartScrub démarre un scrub sur le système de fichiers dont le
// point de montage est fourni dans le corps JSON {"path": "/mnt/pool"}.
func (s *Server) handleStartScrub(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Path string `json:"path"`
	}
	if err := decodeJSON(r, &req); err != nil || req.Path == "" {
		writeError(w, http.StatusBadRequest, "paramètre 'path' (point de montage) requis")
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), btrfs.DefaultTimeout)
	defer cancel()

	if err := btrfs.StartScrub(ctx, req.Path); err != nil {
		s.Logger.Error("démarrage scrub btrfs", "path", req.Path, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de démarrer le scrub")
		return
	}
	w.WriteHeader(http.StatusAccepted)
}

// handleScrubStatus retourne l'état du dernier scrub pour le système de
// fichiers identifié par son point de montage (?path=/mnt/pool).
func (s *Server) handleScrubStatus(w http.ResponseWriter, r *http.Request) {
	mp, ok := mountPointParam(w, r)
	if !ok {
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), btrfs.DefaultTimeout)
	defer cancel()

	status, err := btrfs.GetScrubStatus(ctx, mp)
	if err != nil {
		s.Logger.Error("statut scrub btrfs", "path", mp, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lire le statut du scrub")
		return
	}
	writeJSON(w, http.StatusOK, status)
}
