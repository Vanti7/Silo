# Changelog

Toutes les modifications notables de ce projet sont documentées ici.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et ce projet adhère au [Semantic Versioning](https://semver.org/lang/fr/).

## [Unreleased]

### Added

- `scripts/deploy.sh` : mise à jour d'une instance existante en une
  commande depuis la machine de build (build, transfert, bascule du
  binaire, redémarrage et vérification que le service répond). La version
  précédente est sauvegardée puis restaurée automatiquement si le service
  ne repart pas. La configuration `/etc/silo/silo.env` n'est jamais
  touchée, et l'unité systemd seulement sur `--with-unit`.

- Backend Go (`cmd/silo`) exposant une API REST JSON sous `/api/v1` et
  servant le frontend embarqué.
- Authentification par session opaque (cookie httpOnly), assistant de
  première configuration (création du compte administrateur), gestion des
  utilisateurs et des rôles (`admin` / `user`).
- Tableau de bord système : CPU, mémoire, swap, charge, température,
  uptime, interfaces réseau (via gopsutil).
- Gestion du stockage btrfs : liste des systèmes de fichiers, allocation
  data/metadata, sous-volumes, démarrage et statut de scrub.
- Gestion Docker : liste, démarrage/arrêt/redémarrage/suppression des
  containers, logs, liste des images, informations du daemon.
- Explorateur de fichiers confiné aux racines de partage configurées
  (`os.Root`, aucune traversée de chemin possible) : navigation,
  téléversement, téléchargement, création de dossier, suppression.
- Frontend React/TypeScript/Vite/Tailwind avec thème clair/sombre,
  inspiré de l'interface Synology DSM.
