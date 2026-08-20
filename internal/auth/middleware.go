package auth

import (
	"context"
	"net/http"

	"silo/internal/store"
)

type contextKey int

const userContextKey contextKey = iota

// UserFromContext retourne l'utilisateur authentifié attaché à la requête
// par Middleware, ou nil si la requête n'est pas authentifiée.
func UserFromContext(ctx context.Context) *store.User {
	u, _ := ctx.Value(userContextKey).(*store.User)
	return u
}

// Middleware résout la session à partir du cookie silo_session et injecte
// l'utilisateur correspondant dans le contexte de la requête. Il ne bloque
// pas les requêtes non authentifiées : c'est le rôle de RequireAuth.
func Middleware(st *store.Store) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			cookie, err := r.Cookie(CookieName)
			if err != nil || cookie.Value == "" {
				next.ServeHTTP(w, r)
				return
			}
			user, err := st.GetSessionUser(HashToken(cookie.Value))
			if err != nil {
				next.ServeHTTP(w, r)
				return
			}
			ctx := context.WithValue(r.Context(), userContextKey, user)
			next.ServeHTTP(w, r.WithContext(ctx))
		})
	}
}

// RequireAuth rejette avec 401 toute requête sans utilisateur authentifié.
func RequireAuth(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if UserFromContext(r.Context()) == nil {
			http.Error(w, `{"error":"authentification requise"}`, http.StatusUnauthorized)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// RequireAdmin rejette avec 403 toute requête dont l'utilisateur n'a pas
// le rôle admin. Doit être chaîné après RequireAuth.
func RequireAdmin(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		user := UserFromContext(r.Context())
		if user == nil || user.Role != "admin" {
			http.Error(w, `{"error":"droits administrateur requis"}`, http.StatusForbidden)
			return
		}
		next.ServeHTTP(w, r)
	})
}
