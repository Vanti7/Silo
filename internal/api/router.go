package api

import (
	"io/fs"
	"net/http"

	"silo/internal/auth"
)

// authed protège un handler : accessible à tout utilisateur authentifié.
func authed(fn http.HandlerFunc) http.Handler {
	return auth.RequireAuth(fn)
}

// admin protège un handler : réservé aux utilisateurs de rôle admin.
func admin(fn http.HandlerFunc) http.Handler {
	return auth.RequireAuth(auth.RequireAdmin(fn))
}

// NewRouter construit le multiplexeur HTTP complet : API JSON sous /api/v1
// (chaque route déclare explicitement son niveau de protection) et
// fichiers statiques du frontend (spaFS) pour tout le reste, avec fallback
// vers index.html pour le routage côté client (SPA).
func NewRouter(s *Server, spaFS fs.FS) http.Handler {
	mux := http.NewServeMux()

	// -- Setup et authentification (non protégés) --
	mux.HandleFunc("GET /api/v1/setup/status", s.handleSetupStatus)
	mux.HandleFunc("POST /api/v1/setup", s.handleSetup)
	mux.HandleFunc("POST /api/v1/auth/login", s.handleLogin)
	mux.HandleFunc("POST /api/v1/auth/logout", s.handleLogout)
	mux.HandleFunc("GET /api/v1/auth/me", s.handleMe)
	mux.Handle("PATCH /api/v1/auth/password", authed(s.handleChangePassword))

	// -- Système (lecture, tout utilisateur authentifié) --
	mux.Handle("GET /api/v1/system/stats", authed(s.handleSystemStats))
	mux.Handle("GET /api/v1/system/info", authed(s.handleSystemInfo))

	// -- Stockage btrfs --
	mux.Handle("GET /api/v1/storage/filesystems", authed(s.handleListFilesystems))
	mux.Handle("GET /api/v1/storage/usage", authed(s.handleFilesystemUsage))
	mux.Handle("GET /api/v1/storage/subvolumes", authed(s.handleListSubvolumes))
	mux.Handle("GET /api/v1/storage/scrub", authed(s.handleScrubStatus))
	mux.Handle("POST /api/v1/storage/scrub", admin(s.handleStartScrub))

	// -- Explorateur de fichiers --
	mux.Handle("GET /api/v1/files/roots", authed(s.handleListRoots))
	mux.Handle("GET /api/v1/files/list", authed(s.handleListDir))
	mux.Handle("GET /api/v1/files/download", authed(s.handleDownloadFile))
	mux.Handle("POST /api/v1/files/mkdir", admin(s.handleMkdir))
	mux.Handle("POST /api/v1/files/upload", admin(s.handleUploadFile))
	mux.Handle("POST /api/v1/files/rename", admin(s.handleRenameFile))
	mux.Handle("DELETE /api/v1/files", admin(s.handleDeleteFile))

	// -- Docker --
	mux.Handle("GET /api/v1/docker/containers", authed(s.handleListContainers))
	mux.Handle("GET /api/v1/docker/containers/{id}/logs", authed(s.handleContainerLogs))
	mux.Handle("GET /api/v1/docker/images", authed(s.handleListImages))
	mux.Handle("GET /api/v1/docker/info", authed(s.handleDockerInfo))
	mux.Handle("POST /api/v1/docker/containers/{id}/start", admin(s.handleStartContainer))
	mux.Handle("POST /api/v1/docker/containers/{id}/stop", admin(s.handleStopContainer))
	mux.Handle("POST /api/v1/docker/containers/{id}/restart", admin(s.handleRestartContainer))
	mux.Handle("DELETE /api/v1/docker/containers/{id}", admin(s.handleRemoveContainer))

	// -- Utilisateurs (réservé admin) --
	mux.Handle("GET /api/v1/users", admin(s.handleListUsers))
	mux.Handle("POST /api/v1/users", admin(s.handleCreateUser))
	mux.Handle("PATCH /api/v1/users/{id}", admin(s.handleUpdateUser))
	mux.Handle("DELETE /api/v1/users/{id}", admin(s.handleDeleteUser))

	// -- Frontend statique (SPA) --
	mux.Handle("/", spaHandler(spaFS))

	return auth.Middleware(s.Store)(mux)
}
