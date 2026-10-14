# ADR 0001 — Backend réécrit en Rust

- Statut : accepté
- Date : 2026-10-14

## Contexte

silo est un outil d'administration système : il tourne en root sur le NAS,
pilote `btrfs`, le socket Docker et le système de fichiers des partages,
et reste allumé en permanence sur une machine aux ressources modestes. Le
backend initial était écrit en Go.

## Décision

Le backend est réécrit en Rust, à périmètre fonctionnel constant. Le
frontend React reste tel quel.

Choix techniques :

| Besoin                 | Go (avant)            | Rust (après)                |
|------------------------|-----------------------|-----------------------------|
| HTTP                   | `net/http`            | axum + hyper (tokio)        |
| SQLite                 | `modernc.org/sqlite`  | `rusqlite` (SQLite embarqué)|
| Frontend embarqué      | `go:embed`            | `rust-embed`                |
| Docker                 | SDK officiel          | `bollard`                   |
| Confinement fichiers   | `os.Root`             | `cap-std`                   |
| Métriques système      | gopsutil              | lecture directe de `/proc`  |
| Mots de passe          | `x/crypto/bcrypt`     | `bcrypt` (coût 10 conservé) |

Contraintes de compatibilité, toutes couvertes par des tests :

- même schéma SQLite et même format d'horodatage des sessions
  (`2006-01-02T15:04:05Z`) : une base existante est reprise sans
  migration, sessions ouvertes comprises ;
- hashs bcrypt produits par Go acceptés tels quels ;
- mêmes routes, codes de statut, messages d'erreur et formats JSON ;
- mêmes variables d'environnement, y compris les formes booléennes de Go
  (`1`, `t`, `TRUE`…) et l'adresse d'écoute `:8080` ;
- même délai de lecture des en-têtes HTTP (10 s) et même fermeture
  propre (10 s) ;
- démarrage possible sans daemon Docker joignable.

Le code vise Rust 1.85, version fournie par Debian 13 : le NAS peut
compiler silo avec les seuls paquets de la distribution.

## Conséquences

- Sûreté mémoire garantie à la compilation, sans ramasse-miettes : un
  processus root exposé au réseau gagne à exclure ces classes de bogues.
- Empreinte mémoire plus faible et stable. Mesure sur amd64 (RSS) : au
  repos, 8,0 Mo contre 11,7 Mo pour la version Go ; après 400 requêtes,
  8,5 Mo contre 16,6 Mo.
- La compilation demande un compilateur C (SQLite) et dure plus longtemps
  qu'en Go, surtout la première fois.
- La compilation croisée n'est plus intégrée à la chaîne d'outils :
  depuis une autre machine que le NAS, il faut `cargo-zigbuild` (ou
  compiler sur le NAS).
- Le binaire dépend de la glibc du système au lieu d'être entièrement
  statique ; Debian la fournit toujours.
- Les métriques système ne sont disponibles que sous Linux, seule cible
  de silo.
