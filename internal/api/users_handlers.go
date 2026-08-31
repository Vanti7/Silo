package api

import (
	"errors"
	"net/http"
	"strconv"

	"silo/internal/auth"
	"silo/internal/store"
)

// handleListUsers retourne tous les comptes utilisateurs.
func (s *Server) handleListUsers(w http.ResponseWriter, r *http.Request) {
	users, err := s.Store.ListUsers()
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	out := make([]userResponse, 0, len(users))
	for i := range users {
		out = append(out, toUserResponse(&users[i]))
	}
	writeJSON(w, http.StatusOK, out)
}

type createUserRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
	Role     string `json:"role"`
}

// handleCreateUser crée un nouvel utilisateur (rôle "admin" ou "user").
func (s *Server) handleCreateUser(w http.ResponseWriter, r *http.Request) {
	var req createUserRequest
	if err := decodeJSON(r, &req); err != nil {
		writeError(w, http.StatusBadRequest, "requête invalide")
		return
	}
	if !validUsername(req.Username) || !validPassword(req.Password) {
		writeError(w, http.StatusBadRequest, "identifiant ou mot de passe invalide (mot de passe : 8 caractères minimum)")
		return
	}
	if req.Role != "admin" && req.Role != "user" {
		writeError(w, http.StatusBadRequest, "rôle invalide (admin ou user)")
		return
	}

	hash, err := auth.HashPassword(req.Password)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	id, err := s.Store.CreateUser(req.Username, hash, req.Role)
	if errors.Is(err, store.ErrAlreadyExists) {
		writeError(w, http.StatusConflict, "cet identifiant existe déjà")
		return
	}
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}

	user, err := s.Store.GetUserByID(id)
	if err != nil {
		writeError(w, http.StatusInternalServerError, "erreur interne")
		return
	}
	writeJSON(w, http.StatusCreated, toUserResponse(user))
}

type updateUserRequest struct {
	Password *string `json:"password,omitempty"`
	Role     *string `json:"role,omitempty"`
}

// handleUpdateUser modifie le mot de passe et/ou le rôle d'un utilisateur.
func (s *Server) handleUpdateUser(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.ParseInt(r.PathValue("id"), 10, 64)
	if err != nil {
		writeError(w, http.StatusBadRequest, "identifiant invalide")
		return
	}

	var req updateUserRequest
	if err := decodeJSON(r, &req); err != nil {
		writeError(w, http.StatusBadRequest, "requête invalide")
		return
	}

	if req.Role != nil {
		if *req.Role != "admin" && *req.Role != "user" {
			writeError(w, http.StatusBadRequest, "rôle invalide (admin ou user)")
			return
		}
		if *req.Role != "admin" {
			if err := s.ensureNotLastAdmin(id); err != nil {
				writeError(w, http.StatusConflict, err.Error())
				return
			}
		}
		if err := s.Store.UpdateUserRole(id, *req.Role); err != nil {
			writeStoreErr(w, err)
			return
		}
	}

	if req.Password != nil {
		if !validPassword(*req.Password) {
			writeError(w, http.StatusBadRequest, "mot de passe invalide (8 caractères minimum)")
			return
		}
		hash, err := auth.HashPassword(*req.Password)
		if err != nil {
			writeError(w, http.StatusInternalServerError, "erreur interne")
			return
		}
		if err := s.Store.UpdateUserPassword(id, hash); err != nil {
			writeStoreErr(w, err)
			return
		}
		// Réinitialisation par un administrateur : toutes les sessions du
		// compte concerné sont révoquées, il devra se reconnecter (y compris
		// l'administrateur lui-même s'il réinitialise son propre compte par
		// ce biais plutôt que par /auth/password).
		if err := s.Store.DeleteUserSessions(id); err != nil {
			s.Logger.Error("révocation des sessions après réinitialisation", "user", id, "err", err)
			writeError(w, http.StatusInternalServerError, "erreur interne")
			return
		}
	}

	user, err := s.Store.GetUserByID(id)
	if err != nil {
		writeStoreErr(w, err)
		return
	}
	writeJSON(w, http.StatusOK, toUserResponse(user))
}

// handleDeleteUser supprime un compte utilisateur, sauf s'il s'agit du
// dernier compte administrateur restant.
func (s *Server) handleDeleteUser(w http.ResponseWriter, r *http.Request) {
	id, err := strconv.ParseInt(r.PathValue("id"), 10, 64)
	if err != nil {
		writeError(w, http.StatusBadRequest, "identifiant invalide")
		return
	}

	if err := s.ensureNotLastAdmin(id); err != nil {
		writeError(w, http.StatusConflict, err.Error())
		return
	}

	if err := s.Store.DeleteUser(id); err != nil {
		writeStoreErr(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// ensureNotLastAdmin retourne une erreur si l'utilisateur id est le dernier
// administrateur, afin d'empêcher de se retrouver sans accès admin.
func (s *Server) ensureNotLastAdmin(id int64) error {
	target, err := s.Store.GetUserByID(id)
	if err != nil {
		return err
	}
	if target.Role != "admin" {
		return nil
	}
	n, err := s.Store.CountAdmins()
	if err != nil {
		return err
	}
	if n <= 1 {
		return errors.New("impossible de supprimer ou rétrograder le dernier compte administrateur")
	}
	return nil
}

func writeStoreErr(w http.ResponseWriter, err error) {
	if errors.Is(err, store.ErrNotFound) {
		writeError(w, http.StatusNotFound, "utilisateur introuvable")
		return
	}
	writeError(w, http.StatusInternalServerError, "erreur interne")
}
