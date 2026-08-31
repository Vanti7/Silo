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
scripts/deploy.sh   mise à jour d'une instance (par SSH ou sur place)
scripts/lib/        bascule exécutée sur l'hôte cible, et ses tests
```

## Prérequis

Le binaire compilé est statique et embarque le frontend : **le NAS cible
n'a besoin ni de Go ni de Node/npm**. Ces outils ne servent que sur la
machine où on compile (poste de dev, CI...).

- **Machine de build** : Go ≥ 1.24 (utilise `os.Root`), Node.js ≥ 20 / npm
- **Hôte cible (NAS)** : `btrfs-progs`, `findmnt` (util-linux), Docker —
  uniquement ce dont silo a besoin pour piloter le système, rien pour le
  compiler ou l'exécuter lui-même

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

À exécuter sur la machine de build (poste de dev, CI...), **pas sur le
NAS** — c'est ce script qui a besoin de Node/npm, pas le NAS :

```sh
scripts/build.sh
# -> bin/silo (linux/amd64 par défaut)
```

Ce script compile le frontend dans `web/dist/` puis le binaire Go, qui
l'embarque. Cross-compilation vers une autre cible (ex : NAS ARM64) :
`scripts/build.sh linux arm64`.

## Déploiement (Debian, service systemd)

Depuis la machine de build, copier le binaire vers le NAS :

```sh
scp bin/silo utilisateur@nas:/tmp/silo
```

Puis, sur le NAS (SSH), aucun outil de build requis :

```sh
sudo install -m 0755 /tmp/silo /usr/local/bin/silo
sudo mkdir -p /etc/silo
sudo cp deploy/silo.env.example /etc/silo/silo.env
sudo "$EDITOR" /etc/silo/silo.env   # adapter SILO_SHARE_ROOTS, etc.
sudo cp deploy/silo.service /etc/systemd/system/silo.service
sudo systemctl daemon-reload
sudo systemctl enable --now silo
```

(`deploy/silo.env.example` et `deploy/silo.service` doivent aussi être
présents sur le NAS — les copier avec `scp`, ou cloner le dépôt.)

Le service tourne en root, requis pour piloter `btrfs` (scrub, usage)
et accéder au socket Docker. Par défaut (`SILO_LISTEN_ADDR=:8080` dans
`silo.env.example`) il écoute sur toutes les interfaces et est donc
accessible depuis le LAN à `http://<ip-du-nas>:8080`, cookie de session
en HTTP simple (`SILO_COOKIE_SECURE=false`).

Pour une exposition au-delà du LAN (Internet), mettre `SILO_LISTEN_ADDR`
à `127.0.0.1:8080`, placer un reverse proxy HTTPS (nginx, Caddy...)
devant, et repasser `SILO_COOKIE_SECURE` à `true` — sinon le navigateur
refusera le cookie de session hors HTTPS.

Au premier accès à l'interface, un assistant crée le compte
administrateur (aucun identifiant par défaut n'est fourni).

## Mise à jour

Une fois la première installation faite, les versions suivantes se
déploient en une commande depuis la machine de build :

```sh
scripts/deploy.sh root@nas.local
```

Le script compile, transfère le binaire, sauvegarde la version en place
dans `/usr/local/bin/silo.precedent`, bascule, redémarre le service et
vérifie qu'il répond. **Si le service ne repart pas, la version
précédente est restaurée automatiquement** et la commande sort en erreur.

`/etc/silo/silo.env` n'est jamais modifié. L'unité systemd non plus, sauf
demande explicite :

```sh
scripts/deploy.sh root@nas.local --with-unit   # met aussi à jour silo.service
scripts/deploy.sh root@nas.local --skip-build  # redéploie bin/silo tel quel
scripts/deploy.sh root@nas.local --arch arm64  # NAS ARM
```

Prérequis : un accès SSH sans interaction (clé publique installée) et,
si le compte distant n'est pas root, `sudo` sans mot de passe.

### Depuis le NAS lui-même

Si tu préfères piloter la mise à jour depuis le NAS, transfère d'abord le
binaire compilé (le NAS n'a ni Go ni Node/npm, il ne peut pas le
construire), puis bascule sur place :

```sh
# sur la machine de build
scripts/build.sh
scp bin/silo root@nas.local:/tmp/silo

# sur le NAS
scripts/deploy.sh --local /tmp/silo
```

Sans chemin, `--local` prend `bin/silo` du dépôt. Le mode local fait
exactement la même bascule que le mode distant — sauvegarde, vérification
et retour arrière automatique — puisque les deux exécutent
`scripts/lib/apply-update.sh` sur l'hôte cible.

## Mots de passe oubliés

Aucun flux de réinitialisation non authentifié n'est exposé sur le web :
sans canal de vérification (SMTP), il permettrait à quiconque sur le
réseau de s'emparer du compte administrateur. La récupération passe donc
par l'hôte, où l'accès shell tient lieu de preuve de propriété :

```sh
sudo silo reset-password vanti
```

La commande demande le nouveau mot de passe (saisie masquée) et révoque
les sessions ouvertes du compte. Inutile d'arrêter le service : la base
tolère l'accès concurrent.

Elle lit la base dans `SILO_DATA_DIR` (`/var/lib/silo` par défaut). Si tu
as changé ce chemin dans `/etc/silo/silo.env`, passe-le explicitement,
les variables du service n'étant pas héritées par un shell interactif :

```sh
sudo SILO_DATA_DIR=/chemin/personnalise silo reset-password vanti
```

Depuis l'interface, un utilisateur connecté change son propre mot de
passe dans « Mon compte » (mot de passe actuel exigé), et un
administrateur peut réinitialiser celui de n'importe quel compte depuis
la page Utilisateurs. Dans les deux cas, les sessions du compte concerné
sont révoquées.

## Variables d'environnement

| Variable                   | Défaut               | Description                                             |
|-----------------------------|-----------------------|-----------------------------------------------------------|
| `SILO_LISTEN_ADDR`         | `:8080`               | Adresse d'écoute HTTP                                     |
| `SILO_DATA_DIR`            | `/var/lib/silo`      | Dossier de la base SQLite                                  |
| `SILO_SHARE_ROOTS`         | *(vide)*               | Racines de partage : `nom=chemin,nom2=chemin2`             |
| `SILO_COOKIE_SECURE`       | `true`                | Attribut `Secure` du cookie (mettre à `false` pour un accès HTTP direct sans reverse proxy) |
| `SILO_SESSION_TTL_HOURS`   | `168`                  | Durée de vie d'une session (heures)                        |
| `SILO_DOCKER_HOST`         | *(vide = défaut SDK)*  | Hôte du daemon Docker                                      |

## Tests

```sh
go test ./...                          # backend
bash scripts/lib/apply-update_test.sh  # bascule et retour arrière
```

Les tests de bascule remplacent `systemctl` et `curl` par des doublures
et redirigent les chemins d'installation vers un dossier temporaire : ils
s'exécutent donc sans systemd ni privilèges, y compris sur la machine de
développement.

## Sécurité

- Sessions opaques (jamais de jeton en clair côté serveur), cookie
  `HttpOnly`, `SameSite=Strict`.
- Mots de passe hashés avec bcrypt.
- Commandes système (`btrfs`, `findmnt`) invoquées avec une liste
  d'arguments fixe, jamais via un shell : aucune injection de commande
  possible.
- Explorateur de fichiers confiné par `os.Root` : aucune évasion des
  racines de partage configurées, y compris via des liens symboliques.
