package api

import (
	"io/fs"
	"net/http"
)

// spaHandler sert les fichiers statiques du frontend et retombe sur
// index.html pour toute route inconnue, afin que le routage côté client
// (react-router) fonctionne au rechargement de page.
func spaHandler(spaFS fs.FS) http.Handler {
	fileServer := http.FileServerFS(spaFS)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		path := r.URL.Path
		if path == "/" {
			path = "/index.html"
		}
		if _, err := fs.Stat(spaFS, path[1:]); err != nil {
			// http.FileServer redirige spécifiquement toute requête dont le
			// chemin se termine par "/index.html" (canonicalisation d'URL) ;
			// on cible "/" pour qu'il serve l'index de répertoire sans 301.
			r2 := r.Clone(r.Context())
			r2.URL.Path = "/"
			fileServer.ServeHTTP(w, r2)
			return
		}
		fileServer.ServeHTTP(w, r)
	})
}
