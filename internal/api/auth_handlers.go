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

type changePasswordRequest struct {
	CurrentPassword string `json:"currentPassword"`
	NewPassword     string `json:"newPassword"`
}

// handleChangePassword permet à l'utilisateur authentifié de changer son
// propre mot de passe. La connaissance du mot de passe actuel est exigée :
// un cookie de session volé ne suffit donc pas à s'approprier le compte.
func (s *Server) handleChangePassword(w http.ResponseWriter, r *http.Request) {
	user := auth.UserFromContext(r.Context())
	if user == nil {
		writeError(w, http.StatusUnauthorized, "authentification requise")
		return
	}

	var req changePasswordRequest
	if err := decodeJSON(r, &req); err != nil {
		writeError(w, http.StatusBadRequest, "requête invalide")
		return
	}
	if !auth.VerifyPassword(user.PasswordHash, req.CurrentPassword) {
		writeError(w, http.StatusUnauthorized, "mot de passe actuel incorrect")
		return
	}
	if !validPassword(req.NewPassword) {
		writeError(w, http.StatusBadRequest, "nouveau mot de passe invalide (8 caractères minimum)")
		return
	}

	hash, err := auth.HashPassword(req.NewPassword)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	if err := s.Store.UpdateUserPassword(user.ID, hash); err != nil {
		writeStoreErr(w, err)
		return
	}

	// Toutes les sessions existantes sont révoquées, puis une nouvelle est
	// ouverte pour ce client : l'utilisateur reste connecté ici, mais les
	// éventuelles autres sessions du compte sont invalidées.
	if err := s.Store.DeleteUserSessions(user.ID); err != nil {
		s.Logger.Error("révocation des sessions après changement de mot de passe", "user", user.ID, "err", err)
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	if err := s.startSession(w, user); err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	w.WriteHeader(http.StatusNoContent)
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
	return auth.ValidatePassword(p) == nil
}
