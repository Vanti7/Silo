// Package api expose l'API HTTP REST de silo (JSON) ainsi que le serveur
// de fichiers statiques du frontend embarqué.
package api

import (
	"encoding/json"
	"log/slog"
	"net/http"

	"silo/internal/config"
	"silo/internal/docker"
	"silo/internal/files"
	"silo/internal/store"
)

// Server regroupe les dépendances partagées par les handlers HTTP.
type Server struct {
	Config *config.Config
	Store  *store.Store
	Docker *docker.Client
	Files  *files.Manager
	Logger *slog.Logger
}

// writeJSON sérialise v en JSON dans la réponse avec le code de statut donné.
func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	if v != nil {
		_ = json.NewEncoder(w).Encode(v)
	}
}

// writeError écrit une réponse d'erreur JSON standardisée.
func writeError(w http.ResponseWriter, status int, message string) {
	writeJSON(w, status, map[string]string{"error": message})
}

// decodeJSON décode le corps JSON de la requête dans dst.
func decodeJSON(r *http.Request, dst any) error {
	dec := json.NewDecoder(r.Body)
	dec.DisallowUnknownFields()
	return dec.Decode(dst)
}
