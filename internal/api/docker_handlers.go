package api

import (
	"net/http"

	"github.com/docker/docker/pkg/stdcopy"
)

// handleListContainers retourne tous les containers Docker (démarrés ou non).
func (s *Server) handleListContainers(w http.ResponseWriter, r *http.Request) {
	containers, err := s.Docker.ListContainers(r.Context())
	if err != nil {
		s.Logger.Error("liste containers docker", "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lister les containers")
		return
	}
	writeJSON(w, http.StatusOK, containers)
}

// handleStartContainer démarre un container par son ID.
func (s *Server) handleStartContainer(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	if err := s.Docker.StartContainer(r.Context(), id); err != nil {
		s.Logger.Error("démarrage container", "id", id, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de démarrer le container")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// handleStopContainer arrête un container par son ID.
func (s *Server) handleStopContainer(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	if err := s.Docker.StopContainer(r.Context(), id); err != nil {
		s.Logger.Error("arrêt container", "id", id, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible d'arrêter le container")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// handleRestartContainer redémarre un container par son ID.
func (s *Server) handleRestartContainer(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	if err := s.Docker.RestartContainer(r.Context(), id); err != nil {
		s.Logger.Error("redémarrage container", "id", id, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de redémarrer le container")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// handleRemoveContainer supprime un container par son ID (?force=true pour
// forcer la suppression d'un container en cours d'exécution).
func (s *Server) handleRemoveContainer(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	force := r.URL.Query().Get("force") == "true"
	if err := s.Docker.RemoveContainer(r.Context(), id, force); err != nil {
		s.Logger.Error("suppression container", "id", id, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de supprimer le container")
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// handleContainerLogs diffuse les logs d'un container en flux continu
// (?follow=true) ou en un lot borné (?tail=200, défaut).
func (s *Server) handleContainerLogs(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	follow := r.URL.Query().Get("follow") == "true"
	tail := r.URL.Query().Get("tail")
	if tail == "" {
		tail = "200"
	}

	rc, err := s.Docker.ContainerLogs(r.Context(), id, follow, tail)
	if err != nil {
		s.Logger.Error("logs container", "id", id, "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de récupérer les logs")
		return
	}
	defer rc.Close()

	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.WriteHeader(http.StatusOK)

	// Les logs Docker (containers sans TTY) multiplexent stdout/stderr avec
	// un en-tête binaire de 8 octets par frame ; stdcopy.StdCopy les
	// démultiplexe pour ne restituer que le texte lisible.
	fw := flushWriter{w: w}
	fw.f, _ = w.(http.Flusher)
	_, _ = stdcopy.StdCopy(fw, fw, rc)
}

// flushWriter force un flush HTTP après chaque écriture, nécessaire pour
// diffuser les logs en flux continu (?follow=true) sans attendre la fin.
type flushWriter struct {
	w http.ResponseWriter
	f http.Flusher
}

func (fw flushWriter) Write(p []byte) (int, error) {
	n, err := fw.w.Write(p)
	if fw.f != nil {
		fw.f.Flush()
	}
	return n, err
}

// handleListImages retourne les images Docker locales.
func (s *Server) handleListImages(w http.ResponseWriter, r *http.Request) {
	images, err := s.Docker.ListImages(r.Context())
	if err != nil {
		s.Logger.Error("liste images docker", "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lister les images")
		return
	}
	writeJSON(w, http.StatusOK, images)
}

// handleDockerInfo retourne un résumé de l'état du daemon Docker.
func (s *Server) handleDockerInfo(w http.ResponseWriter, r *http.Request) {
	info, err := s.Docker.GetInfo(r.Context())
	if err != nil {
		s.Logger.Error("info daemon docker", "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de contacter le daemon Docker")
		return
	}
	writeJSON(w, http.StatusOK, info)
}
