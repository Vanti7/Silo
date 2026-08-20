// Package web embarque les fichiers statiques compilés du frontend
// (web/frontend, build Vite) dans le binaire Go via go:embed, pour un
// déploiement en un seul exécutable.
package web

import (
	"embed"
	"io/fs"
)

//go:embed all:dist
var distFS embed.FS

// DistFS retourne le contenu de web/dist (build du frontend) sans le
// préfixe "dist", prêt à être servi tel quel par http.FileServerFS.
func DistFS() fs.FS {
	sub, err := fs.Sub(distFS, "dist")
	if err != nil {
		panic("web: dist embarqué invalide: " + err.Error())
	}
	return sub
}
