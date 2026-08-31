# Changelog

Toutes les modifications notables de ce projet sont documentées ici.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/),
et ce projet adhère au [Semantic Versioning](https://semver.org/lang/fr/).

## [Unreleased]

### Added

- Réinitialisation de mot de passe :
  - sous-commande `silo reset-password <utilisateur>`, chemin de
    récupération en cas de perte du mot de passe administrateur (l'accès
    shell à l'hôte tient lieu de preuve de propriété) ;
  - page « Mon compte » permettant à tout utilisateur connecté de changer
    son propre mot de passe, la saisie du mot de passe actuel étant exigée ;
  - bouton de réinitialisation sur la page Utilisateurs, pour qu'un
    administrateur redéfinisse le mot de passe de n'importe quel compte.
- Révocation des sessions à chaque changement de mot de passe : un cookie
  obtenu avant le changement cesse de fonctionner. La session à l'origine
  du changement reste active.

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
