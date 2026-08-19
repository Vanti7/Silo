package api

import (
	"net/http"

	"silo/internal/system"
)

// handleSystemStats retourne un instantané des métriques système courantes.
func (s *Server) handleSystemStats(w http.ResponseWriter, r *http.Request) {
	stats, err := system.Collect()
	if err != nil {
		s.Logger.Error("collecte des métriques système", "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lire les métriques système")
		return
	}
	writeJSON(w, http.StatusOK, stats)
}

// handleSystemInfo retourne les informations statiques de l'hôte.
func (s *Server) handleSystemInfo(w http.ResponseWriter, r *http.Request) {
	info, err := system.CollectInfo()
	if err != nil {
		s.Logger.Error("collecte des informations système", "err", err)
		writeError(w, http.StatusInternalServerError, "impossible de lire les informations système")
		return
	}
	writeJSON(w, http.StatusOK, info)
}
