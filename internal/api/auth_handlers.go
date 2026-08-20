package api

import (
	"errors"
	"net/http"
	"time"

	"silo/internal/auth"
	"silo/internal/store"
)

type credentials struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

type userResponse struct {
	ID       int64  `json:"id"`
	Username string `json:"username"`
	Role     string `json:"role"`
}

func toUserResponse(u *store.User) userResponse {
	return userResponse{ID: u.ID, Username: u.Username, Role: u.Role}
}

// handleSetupStatus indique si le premier compte administrateur reste à créer.
func (s *Server) handleSetupStatus(w http.ResponseWriter, r *http.Request) {
	n, err := s.Store.CountUsers()
	if err != nil {
		writeError(w, http.StatusInternalServerError, "impossible de vérifier l'état d'initialisation")
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"needsSetup": n == 0})
}

// handleSetup crée le premier compte administrateur. N'est autorisé que
// tant qu'aucun utilisateur n'existe, ce qui empêche toute création
// ultérieure non authentifiée.
func (s *Server) handleSetup(w http.ResponseWriter, r *http.Request) {
	n, err := s.Store.CountUsers()
	if err != nil {
		writeError(w, http.StatusInternalServerError, "impossible de vérifier l'état d'initialisation")
		return
	}
	if n > 0 {
		writeError(w, http.StatusConflict, "un compte administrateur existe déjà")
		return
	}

	var creds credentials
	if err := decodeJSON(r, &creds); err != nil {
		writeError(w, http.StatusBadRequest, "requête invalide")
		return
	}
	if !validUsername(creds.Username) || !validPassword(creds.Password) {
		writeError(w, http.StatusBadRequest, "identifiant ou mot de passe invalide (mot de passe : 8 caractères minimum)")
		return
	}

	hash, err := auth.HashPassword(creds.Password)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	userID, err := s.Store.CreateUser(creds.Username, hash, "admin")
	if err != nil {
		writeError(w, http.StatusInternalServerError, "création du compte impossible")
		return
	}

	user, err := s.Store.GetUserByID(userID)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	if err := s.startSession(w, user); err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	writeJSON(w, http.StatusCreated, toUserResponse(user))
}

// handleLogin authentifie un utilisateur existant et ouvre une session.
func (s *Server) handleLogin(w http.ResponseWriter, r *http.Request) {
	var creds credentials
	if err := decodeJSON(r, &creds); err != nil {
		writeError(w, http.StatusBadRequest, "requête invalide")
		return
	}

	user, err := s.Store.GetUserByUsername(creds.Username)
	if errors.Is(err, store.ErrNotFound) {
		writeError(w, http.StatusUnauthorized, "identifiants invalides")
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	if !auth.VerifyPassword(user.PasswordHash, creds.Password) {
		writeError(w, http.StatusUnauthorized, "identifiants invalides")
		return
	}

	if err := s.startSession(w, user); err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	writeJSON(w, http.StatusOK, toUserResponse(user))
}

// handleLogout invalide la session courante.
func (s *Server) handleLogout(w http.ResponseWriter, r *http.Request) {
	if cookie, err := r.Cookie(auth.CookieName); err == nil {
		_ = s.Store.DeleteSession(auth.HashToken(cookie.Value))
	}
	http.SetCookie(w, &http.Cookie{
		Name:     auth.CookieName,
		Value:    "",
		Path:     "/",
		MaxAge:   -1,
		HttpOnly: true,
		Secure:   s.Config.SessionCookieSecure,
		SameSite: http.SameSiteStrictMode,
	})
	w.WriteHeader(http.StatusNoContent)
}

// handleMe retourne l'utilisateur authentifié courant.
func (s *Server) handleMe(w http.ResponseWriter, r *http.Request) {
	user := auth.UserFromContext(r.Context())
	if user == nil {
		writeError(w, http.StatusUnauthorized, "authentification requise")
		return
	}
	writeJSON(w, http.StatusOK, toUserResponse(user))
}

// startSession génère un jeton de session, le persiste et pose le cookie.
func (s *Server) startSession(w http.ResponseWriter, user *store.User) error {
	token, err := auth.NewSessionToken()
	if err != nil {
		return err
	}
	ttl := time.Duration(s.Config.SessionTTLHours) * time.Hour
	if err := s.Store.CreateSession(auth.HashToken(token), user.ID, ttl); err != nil {
		return err
	}
	http.SetCookie(w, &http.Cookie{
		Name:     auth.CookieName,
		Value:    token,
		Path:     "/",
		MaxAge:   int(ttl.Seconds()),
		HttpOnly: true,
		Secure:   s.Config.SessionCookieSecure,
		SameSite: http.SameSiteStrictMode,
	})
	return nil
}

func validUsername(u string) bool {
	return len(u) >= 3 && len(u) <= 64
}

func validPassword(p string) bool {
	return len(p) >= 8 && len(p) <= 256
}
