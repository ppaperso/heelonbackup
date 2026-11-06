---
name: rust-dev
description: Conventions, architecture et workflow de développement de HeelonBackup (Rust, sauvegarde Linux → NAS Synology via SMB). À charger AVANT toute tâche de code dans ce dépôt — implémentation, refactoring, tests, correction de bug, revue, mise à jour de dépendances, documentation, commit ou PR — et dès qu'il est question de la CLI, de la console interactive, du justfile, de smbclient ou du NAS.
---

# HeelonBackup — conventions de développement

Ce fichier est versionné dans un dépôt public : il obéit lui-même aux règles d'hygiène ci-dessous.

## Projet

Outil de sauvegarde fiable, Linux → Synology NAS (BeeStation) via SMB.
Rust **édition 2024**, **MSRV 1.99** (`rust-version` dans `Cargo.toml`), licence Apache-2.0. Deux interfaces : une CLI complète (`heelonbackup <sous-commande>`) et une **console interactive** (`heelonbackup` sans argument).

## Architecture

    src/
    ├── main.rs            # panic hook (stdout fermé → exit 141), délègue à cli::run()
    ├── lib.rs             # exports de la bibliothèque
    ├── error.rs           # BackupError / ConfigError (thiserror), BackupError::io(ctx, src)
    ├── config.rs          # config JSON, environnement dev/prod (HEELONBACKUP_ENV), mot de passe
    ├── ui.rs              # say! (sortie service → stdout ou console), PROGRESS, LogWriter,
    │                      # active_bars, bytes/duration/rate/…, bannière
    ├── backup/            # scanner.rs (scan + exclusions globset), hasher.rs (SHA-256)
    ├── storage/           # smb_client.rs (+ probe TCP du NAS), transfer.rs, backup_manifest.rs
    ├── service/           # orchestration partagée CLI/console : backup, restore, verify,
    │                      # status, config, remote (connexion SMB + manifestes distants) ;
    │                      # mod.rs : enum Exit (0 ok, 1 échec, 2 avertissements, 130 interrompu),
    │                      # interrupted() / request_interrupt() (arrêt des transferts)
    ├── cli/               # clap ; une commande par fichier : backup, restore, verify,
    │                      # status, config_cmd, common ; run() async dans mod.rs
    └── console/           # tableau de bord ratatui : mod.rs (boucle, terminal, sonde NAS),
                           # app.rs (état + touches, pur), view.rs (rendu), prompt.rs
                           # (dialogues flux ↔ dashboard), un fichier par flux,
                           # strings.rs (libellés EN/FR), validate.rs (validateurs purs)

Règles de placement :

- Nouvelle sous-commande CLI = nouveau fichier dans `src/cli/`, déclaré et câblé dans le Parser clap de `src/cli/mod.rs`.
- **`cli/` et `console/` ne contiennent aucune logique métier** : la CLI analyse les arguments, la console collecte les choix, et toutes deux appellent la même orchestration `service/`. Logique pure → `backup/`/`storage/` ; validation pure des saisies console → `console/validate.rs`.
- Les fonctions `service/` renvoient un `Exit` ; la conversion en code de sortie se fait uniquement dans le front-end.
- **Parité CLI ↔ console** : toute nouvelle fonctionnalité est exposée en commande CLI **et** en flux console (sinon, exception justifiée dans la PR). Les surcharges purement techniques (`--workers`, `--max-file-size`, `--exclude`) restent CLI-only.
- **Aucun `println!` dans `service/`** : toute sortie utilisateur passe par `ui::say!` (stdout en CLI, panneau Terminal en console). Toute progression passe par `ui::spinner` / `ui::transfer_bar` (lues par la console via `ui::active_bars`) et les logs par `ui::LogWriter`.
- Tout transfert interruptible attend `service::interrupted()`, jamais `tokio::signal::ctrl_c()` directement (en mode raw, Ctrl-C est une touche, pas un SIGINT).

## Idiomes du projet

- **Erreurs** : `thiserror` dans `error.rs`. Erreurs utilisateur = variantes de `BackupError`/`ConfigError`, message **anglais, minuscule, orienté action** (ex. `cannot reach the NAS: {0}`). Toujours du contexte : `BackupError::io(context, source)` plutôt qu'un `#[from] io::Error` brut. Pas d'`anyhow` dans le code (la dépendance n'est utilisée nulle part).
- **Async** : `#[tokio::main]`, smbclient en processus enfant (`tokio::process`), parallélisme via `backup.workers`. Le travail CPU/bloquant (hash, éditeur externe) tourne dans `spawn_blocking` ; l'interruption passe par `tokio::select!` contre `service::interrupted()`.
- **Sérialisation** : `serde` dérivé, JSON via `serde_json`. Config prod : `~/.config/heelonbackup/config.json` (dev : `.heelonbackup/config.json`). Clé `language: "en"|"fr"` (défaut `en`), rejetée si invalide.
- **Formatage terminal** : helpers de `ui.rs` (`bytes`, `duration`, `rate`, `estimate`, `spinner`, `transfer_bar`). Ne pas réimplémenter.
- **Doc** : `//!` en tête de module, `///` sur les éléments publics, en anglais, avec le « pourquoi ».
- **Édition 2024** : idiomes modernes (`LazyLock`, `IsTerminal`, `let-else`). Pas de `unsafe`.

## Console interactive

- **Lancement** : sans sous-commande → tableau de bord plein écran ; tout argument → CLI inchangée. stdin/stdout non-tty refusés proprement (exit 1) ; quitter → exit 0.
- **Crates** : `ratatui` (backend crossterm réexporté : `ratatui::crossterm`, pas de dépendance crossterm directe). Plus de dialoguer. `rpassword` reste réservé à la CLI : en console, aucun prompt sur stdin/stdout (l'écran alterné le casserait).
- **Écran** : en-tête (version, DEV/PROD, date/heure, EN|FR), panneau NAS (`service::remote::reachability`, toutes les 15 s et après chaque opération), panneau dernière sauvegarde (`LocalHistory`), actions avec raccourcis, panneau Terminal (sortie `ui::say!` + barres actives), aide des touches. Les flèches agissent sur le panneau qui a le focus (`app::Focus`, Tab/←/→) : toute nouvelle zone navigable doit s'intégrer à ce modèle, jamais capturer les flèches globalement.
- **Flux** : chaque flux tourne dans sa propre tâche et garde un code linéaire ; ses questions passent par `prompt::{select, confirm, input, password}` (boîtes modales). Un programme externe (éditeur) passe par `prompt::suspended`.
- **Mot de passe** : demandé une seule fois par session via `console::ensure_password` (boîte masquée) avant toute opération NAS, gardé en mémoire (`config::set_session_password`), oublié sur `SmbAuth` et à la fermeture.
- **Bilingue** : libellés dans `src/console/strings.rs` (struct `Strings`, instances EN/FR), bascule `L`, persistée dans `language`. CLI, erreurs, logs et sortie du panneau Terminal restent en anglais. Tout nouveau libellé est ajouté dans **les deux** langues.
- **Sécurité par défaut** : les flux backup et restore proposent **toujours un dry-run avant tout transfert/écrasement réel**, exécution uniquement sur confirmation explicite.
- **Annulation** : dans une boîte, Échap / Ctrl-C → `Ok(None)` (retour au menu). Opération en cours : Ctrl-C → `service::request_interrupt()`, le tableau de bord reste ouvert. Au repos : q / Échap / Ctrl-C quittent. Jamais de panic ni de terminal cassé (`ratatui::init/restore`).
- **Tests** : la logique reste testable sans terminal : `app.rs` (touches, dialogues, défilement), `view.rs` via `ratatui::backend::TestBackend` (rendu, mot de passe jamais affiché), validateurs. Validation réelle : piloter le binaire dans une session tmux dédiée (`tmux -L <socket>`, `capture-pane`), en dev, sans transfert réel.
- **Mode dev** : la console respecte `HEELONBACKUP_ENV=dev` et la bannière `ui::print_environment_banner()`.

## Sécurité — à respecter strictement

- **Jamais de mot de passe SMB en clair** dans le code, la config commitée, les logs, les messages d'erreur ou une ligne de commande. Il vient de `HEELONBACKUP_SMB_PASSWORD`, d'un prompt `rpassword` (CLI) ou de la boîte masquée de la console (mémoire uniquement).
- **Jamais de sauvegarde réelle hors dev** : toutes les recettes `just` fixent `HEELONBACKUP_ENV=dev` (config/historique dans `.heelonbackup/`, dossiers `dev_*` sur le NAS). Ne jamais lancer `cargo run` / le binaire directement pour une opération NAS : passer par `just`.
- Tests réseau réels : uniquement si l'utilisateur a fourni `HEELONBACKUP_SMB_PASSWORD` dans le shell, et uniquement via `just verify-deep`, `just backup-execute` (petites sources de test) et `just restore-dryrun`. Jamais en production, jamais hors des dossiers `dev_*`.
- **Recettes interdites à l'agent** (personnelles, sudo ou transfert réel non-dev) : `backup-laptop*`, `system-inventory`, `setup-patrick`, `backup-patrick*`, `quick-test`, `full-test`. Les laisser à l'utilisateur.
- Ne jamais lire, modifier ni committer `.env`, `~/.config/heelonbackup/`, `.heelonbackup/` ni aucun secret réel.
- Consulter `SECURITY.md` et `SECRETS_GUIDE.md` avant tout ce qui touche à l'authentification SMB ou aux variables d'environnement.

## Workflow de développement

Toujours via le justfile (il fixe `HEELONBACKUP_ENV=dev`) :

    just dev              # build + test + clippy + fmt --check
    just test             # ou ciblé : just test-scanner | test-hasher | test-transfer
    just fmt-fix          # corriger le formatage
    just audit            # cargo audit
    just run --help       # args passés tels quels au binaire (pas de `--` intermédiaire)
    just run              # la console, dans un vrai terminal (pas un pipe)

**`just dev` est moins strict que la CI** (son clippy n'échoue pas sur les warnings). Avant tout commit, lancer aussi la commande exacte de la CI :

    cargo clippy --all-targets --all-features -- -D warnings

Ordre pour une tâche de code :

1. Lire les fichiers concernés avant d'écrire.
2. Implémenter selon l'architecture et les idiomes ci-dessus.
3. Tests dans le module (`#[cfg(test)] mod tests` en fin de fichier), cas limites compris (chemins vides, gros fichiers, interruptions, erreurs SMB simulées).
4. `just fmt-fix`, puis `just dev` **et** le clippy CI jusqu'au vert, sans warning.
5. Si un comportement utilisateur change : `just run --help`, `just backup-dryrun <source>`. Pour toute modification de CLI, comparer sorties et codes de sortie avant/après (méthode de régression de référence). La console ne se teste pas dans un pipe : la piloter dans tmux (voir « Console interactive ») ou demander à l'utilisateur de lancer `just run`.

## Validation de sauvegarde (mode dev uniquement)

- `just backup-dryrun <source>` — scan sans connexion NAS, sans hash ni écriture.
- `just backup-execute <source>` — transfert réel en dev.
- `just status-detailed`, `just verify <backup>`, `just verify-deep <backup>` — intégrité et SHA-256.
- `just restore-dryrun <backup> <target>` / `just restore-execute <backup> <target>`.

Toute modification de `scanner.rs`, `hasher.rs`, `transfer.rs`, `backup_manifest.rs`, `smb_client.rs` ou d'un module `service/` est validée par au moins un dry-run ; toute modification du hasher par `just test-hasher`.

## Maintenance des dépendances — le projet doit rester à jour

Une fois par session de travail (et systématiquement avant une PR) :

1. Lister les mises à jour : `cargo update --dry-run --verbose` (compatibles + versions majeures disponibles), puis `just audit`. (`cargo outdated`/`cargo upgrade` ne sont pas installés : ne pas les supposer présents.)
2. **Patch/mineure** : `cargo update` sans discussion. Pour relever une contrainte dans `Cargo.toml`, éditer la version à la main.
3. **Majeure** : lire le changelog, adapter le code aux breaking changes, puis `just dev` + clippy CI + `just audit` + tests ciblés du module touché. Ne jamais laisser une migration à moitié faite : la terminer dans le même commit `chore(deps):` ou la signaler comme tâche séparée avec les breaking changes identifiés.
4. `Cargo.lock` reste commité. Le **MSRV 1.99** doit tenir : une crate exigeant `rust-version` > 1.99 est refusée ou soumise à décision explicite de l'utilisateur (la CI compile en `stable` et ne le vérifie pas).
5. Commit dédié `chore(deps): …`, jamais mélangé avec du fonctionnel.
6. Vulnérabilité connue (`cargo audit` rouge) : la mise à jour passe **avant** toute autre tâche.

## Documentation

- Commande, option ou flux modifié → `docs/cli_reference.md` **et** `docs/user_guide.md` + `docs/user_guide_fr.md` (toujours synchronisés EN/FR), plus `README.md` si pertinent. Évolution console → section « Mode interactif » des deux guides.
- Nouvelle clé de configuration → `docs/configuration.md` et `config.example.json` (valeurs d'exemple uniquement).
- **La doc décrit le code réel** : en cas d'écart doc/code, le signaler à l'utilisateur avec une recommandation (implémenter vs corriger la doc), ne jamais laisser les deux diverger silencieusement.

## Hygiène du dépôt public — en permanence

Projet open source : **aucune information personnelle réelle**.

- **IP du NAS et de tout équipement** : exclusivement `192.168.1.100` (IP d'exemple) dans docs, exemples et commentaires.
- **Secrets** : jamais de mot de passe, token ou credential réel, même en commentaire. Placeholders : `your_password`, `your_username`.
- **Chemins personnels** : le chemin du home réel n'est toléré que dans les recettes personnelles du justfile (celles listées comme interdites à l'agent). Tout nouvel exemple utilise `/home/user` ou `~`.
- **Avant tout commit** : relire `git status` et `git diff --staged` ; jamais de `git add -A` sans relecture. `.env`, `.heelonbackup/` et `.claude/settings.local.json` ne doivent jamais être suivis.
- Toute information personnelle découverte est corrigée dans le même commit, ou escaladée immédiatement.

## Commits et pull requests (cf. `CONTRIBUTING.md`)

- **Langue** : commits, PR, issues et commentaires de code **en anglais uniquement**.
- **Conventional Commits** : `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:` (`chore(deps):` pour les dépendances). Un seul changement logique par commit, pas de commit « WIP ».
- Branches : `feature/<nom>` ou `fix/<nom>`, jamais de travail direct sur `main`. PR : titre préfixé, description suivant `.github/PULL_REQUEST_TEMPLATE.md`, CI verte (build, test, fmt, clippy `-D warnings`). Fusion par merge commit, pas de squash.
- Revue par au moins un autre contributeur et commentaires traités avant fusion.
- Après fusion : CI vérifiée sur `main`, branches locales supprimées (`git branch -d`, `git remote prune origin`). Tout changement visible de l'utilisateur → bump de version dans `Cargo.toml`, release notes et tag `vX.Y.Z` (le tag déclenche le job `release` de la CI).
