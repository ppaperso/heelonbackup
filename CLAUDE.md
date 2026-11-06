# HeelonBackup

Sauvegarde Linux → NAS Synology via SMB, Rust édition 2024, MSRV 1.99. CLI (`heelonbackup <cmd>`) + console interactive (sans argument) ; toute la logique métier vit dans `src/service/`.

- **Avant tout commit** : `just dev` **et** `cargo clippy --all-targets --all-features -- -D warnings` (la CI est plus stricte que `just dev`).
- **Environnement dev obligatoire** : toute opération passe par `just` (`HEELONBACKUP_ENV=dev`, dossiers `dev_*` sur le NAS) ; jamais de sauvegarde en production.
- **Recettes interdites à l'agent** : `backup-laptop*`, `system-inventory`, `setup-patrick`, `backup-patrick*`, `quick-test`, `full-test`.
- **Dépôt public** : aucun secret, aucune IP réelle (exemple : `192.168.1.100`), aucun chemin personnel ; commits en anglais, Conventional Commits.

Détail complet (architecture, idiomes, console, sécurité, dépendances, docs, PR) : skill `rust-dev` dans [.claude/skills/rust-dev/SKILL.md](.claude/skills/rust-dev/SKILL.md).
