# silo

Interface web d'administration pour NAS maison (Debian + pool btrfs +
Docker), dans l'esprit de Synology DSM : tableau de bord système,
stockage btrfs, gestion des containers Docker, explorateur de fichiers,
utilisateurs.

Déployée comme un binaire Go unique (le frontend est embarqué au
build) tournant en service systemd sur l'hôte.

## Architecture

- **Backend** : Go, API REST JSON sous `/api/v1` (`net/http`, sans
  framework). Persistance des comptes/sessions en SQLite
  (`modernc.org/sqlite`, pur Go, sans cgo).
- **Frontend** : React + TypeScript + Vite + Tailwind CSS
  (`web/frontend`), compilé et embarqué dans le binaire via `go:embed`
  (`web/embed.go`).
- **Stockage** : interrogation de `btrfs` (CLI) et `findmnt`, aucune
  bibliothèque tierce mature n'existant pour cet usage.
- **Docker** : SDK officiel `github.com/docker/docker/client`.
- **Fichiers** : confinement via `os.Root` (Go 1.24+), qui exclut toute
  traversée de chemin (`../..`) au niveau du système d'exploitation.

```
cmd/silo/         point d'entrée
internal/api/      handlers HTTP + routage
internal/auth/     sessions, hashage des mots de passe
internal/store/    persistance SQLite (utilisateurs, sessions)
internal/system/   métriques hôte (CPU, mémoire, réseau, température)
internal/btrfs/    interrogation des systèmes de fichiers btrfs
internal/docker/   client Docker
internal/files/    explorateur de fichiers confiné
web/frontend/       code source du frontend (React/TS/Vite)
web/dist/           build du frontend, embarqué par web/embed.go
deploy/             unité systemd, exemple de fichier d'environnement
scripts/build.sh    build complet (frontend + backend)
```

## Prérequis

- Go ≥ 1.24 (utilise `os.Root`)
- Node.js ≥ 20 / npm (pour le frontend)
- Sur l'hôte cible : `btrfs-progs`, `findmnt` (util-linux), Docker

## Développement

Backend seul (le frontend embarqué reste celui du dernier build) :

```sh
go run ./cmd/silo
```

Frontend en mode développement (proxy vers le backend sur `:8080`) :

```sh
cd web/frontend
npm install
npm run dev
```

## Build de production

```sh
scripts/build.sh
# -> bin/silo (linux/amd64 par défaut)
```

Ce script compile le frontend dans `web/dist/` puis le binaire Go, qui
l'embarque. Cross-compilation vers une autre cible :
`scripts/build.sh linux arm64`.

## Déploiement (Debian, service systemd)

```sh
sudo install -m 0755 bin/silo /usr/local/bin/silo
sudo mkdir -p /etc/silo
sudo cp deploy/silo.env.example /etc/silo/silo.env
sudo "$EDITOR" /etc/silo/silo.env   # adapter SILO_SHARE_ROOTS, etc.
sudo cp deploy/silo.service /etc/systemd/system/silo.service
sudo systemctl daemon-reload
sudo systemctl enable --now silo
```

Le service tourne en root, requis pour piloter `btrfs` (scrub, usage)
et accéder au socket Docker. Il écoute par défaut sur `127.0.0.1:8080` ;
exposer via un reverse proxy HTTPS (nginx, Caddy...) pour un accès
distant.

Au premier accès à l'interface, un assistant crée le compte
administrateur (aucun identifiant par défaut n'est fourni).

## Variables d'environnement

| Variable                   | Défaut               | Description                                             |
|-----------------------------|-----------------------|-----------------------------------------------------------|
| `SILO_LISTEN_ADDR`         | `:8080`               | Adresse d'écoute HTTP                                     |
| `SILO_DATA_DIR`            | `/var/lib/silo`      | Dossier de la base SQLite                                  |
| `SILO_SHARE_ROOTS`         | *(vide)*               | Racines de partage : `nom=chemin,nom2=chemin2`             |
| `SILO_COOKIE_SECURE`       | `true`                | Attribut `Secure` du cookie de session (désactiver en HTTP dev) |
| `SILO_SESSION_TTL_HOURS`   | `168`                  | Durée de vie d'une session (heures)                        |
| `SILO_DOCKER_HOST`         | *(vide = défaut SDK)*  | Hôte du daemon Docker                                      |

## Tests

```sh
go test ./...
```

## Sécurité

- Sessions opaques (jamais de jeton en clair côté serveur), cookie
  `HttpOnly`, `SameSite=Strict`.
- Mots de passe hashés avec bcrypt.
- Commandes système (`btrfs`, `findmnt`) invoquées avec une liste
  d'arguments fixe, jamais via un shell : aucune injection de commande
  possible.
- Explorateur de fichiers confiné par `os.Root` : aucune évasion des
  racines de partage configurées, y compris via des liens symboliques.
