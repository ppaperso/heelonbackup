//! Bilingual (EN/FR) labels of the interactive console
//!
//! The rest of the application (CLI, errors, logs) stays in English; only the
//! console chrome is bilingual (header, panels, menu, dialogs). The service
//! output shown in the terminal pane stays in English, like the CLI. The active
//! language is stored in the configuration file (`language`: "en" | "fr",
//! default "en") and toggled with the `L` key (« Langue / Language »).

use crate::storage::BackupStatus;

/// Console UI language
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    En,
    Fr,
}

impl Lang {
    /// From the `language` configuration value; anything else falls back to English
    pub(crate) fn from_config(value: &str) -> Self {
        match value {
            "fr" => Self::Fr,
            _ => Self::En,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
        }
    }

    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::En => Self::Fr,
            Self::Fr => Self::En,
        }
    }

    pub(crate) fn strings(self) -> &'static Strings {
        match self {
            Self::En => &EN,
            Self::Fr => &FR,
        }
    }
}

/// Console labels, in one language
pub(crate) struct Strings {
    pub lang: Lang,
    // header and panels
    pub env_dev: &'static str,
    pub env_prod: &'static str,
    pub panel_nas: &'static str,
    pub panel_last: &'static str,
    pub panel_actions: &'static str,
    pub panel_terminal: &'static str,
    pub nas_online: &'static str,
    pub nas_offline: &'static str,
    pub nas_checking: &'static str,
    pub nas_no_config: &'static str,
    pub field_share: &'static str,
    pub field_folder: &'static str,
    pub field_sources: &'static str,
    pub field_retention: &'static str,
    pub field_encryption: &'static str,
    pub field_status: &'static str,
    pub field_date: &'static str,
    pub field_size: &'static str,
    pub field_duration: &'static str,
    pub field_last_ok: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub files: &'static str,
    pub no_history: &'static str,
    pub stale_warning: &'static str,
    pub partial: &'static str,
    pub status_completed: &'static str,
    pub status_with_errors: &'static str,
    pub status_failed: &'static str,
    pub status_in_progress: &'static str,
    pub terminal_empty: &'static str,
    pub keys_idle: &'static str,
    pub keys_busy: &'static str,
    pub keys_prompt: &'static str,
    pub keys_terminal: &'static str,
    pub keep_all: &'static str,
    pub running: &'static str,
    pub busy_refused: &'static str,
    pub interrupt_requested: &'static str,
    pub result_ok: &'static str,
    pub result_warnings: &'static str,
    pub result_failed: &'static str,
    pub result_interrupted: &'static str,
    pub error: &'static str,
    pub password_prompt: &'static str,
    pub password_rejected: &'static str,
    pub editor_running: &'static str,
    // main menu
    pub menu_backup: &'static str,
    pub menu_status: &'static str,
    pub menu_list: &'static str,
    pub menu_verify: &'static str,
    pub menu_restore: &'static str,
    pub menu_seal: &'static str,
    pub menu_config: &'static str,
    pub menu_language: &'static str,
    pub menu_quit: &'static str,
    pub language_saved: &'static str,
    // flows
    pub status_prompt: &'static str,
    pub status_options: [&'static str; 4],
    pub history_clear_prompt: &'static str,
    pub config_prompt: &'static str,
    pub config_options: [&'static str; 6],
    pub yes: &'static str,
    pub no: &'static str,
    pub backup_source_prompt: &'static str,
    pub backup_source_options: [&'static str; 3],
    pub backup_paths_prompt: &'static str,
    pub backup_list_prompt: &'static str,
    pub backup_dryrun_prompt: &'static str,
    pub backup_run_prompt: &'static str,
    pub seal_run_prompt: &'static str,
    pub seal_action_prompt: &'static str,
    pub seal_options: [&'static str; 4],
    pub sealed_backup_prompt: &'static str,
    pub seal_source_prompt: &'static str,
    pub seal_archive_prompt: &'static str,
    pub seal_staging_prompt: &'static str,
    pub seal_save_prompt: &'static str,
    pub seal_secret_warning: &'static str,
    pub seal_password_prompt: &'static str,
    pub seal_password_confirm: &'static str,
    pub verify_prompt: &'static str,
    pub verify_latest: &'static str,
    pub verify_deep_prompt: &'static str,
    pub no_backup_on_nas: &'static str,
    pub restore_prompt: &'static str,
    pub restore_latest: &'static str,
    pub restore_target_prompt: &'static str,
    pub restore_paths_prompt: &'static str,
    pub restore_overwrite_prompt: &'static str,
    pub restore_run_prompt: &'static str,
}

impl Strings {
    /// Localized backup status
    pub(crate) fn status(&self, status: BackupStatus) -> &'static str {
        match status {
            BackupStatus::Completed => self.status_completed,
            BackupStatus::CompletedWithErrors => self.status_with_errors,
            BackupStatus::Failed => self.status_failed,
            BackupStatus::InProgress => self.status_in_progress,
        }
    }

    /// `5 min ago` / `il y a 5 min` from an age in seconds
    pub(crate) fn age(&self, secs: i64) -> String {
        let secs = secs.max(0);
        let (n, unit_en, unit_fr) = match secs {
            0..60 => {
                return match self.lang {
                    Lang::En => "just now".into(),
                    Lang::Fr => "à l'instant".into(),
                };
            }
            60..3600 => (secs / 60, "min", "min"),
            3600..86400 => (secs / 3600, "h", "h"),
            _ => (secs / 86400, "days", "jours"),
        };
        match self.lang {
            Lang::En => format!("{n} {unit_en} ago"),
            Lang::Fr => format!("il y a {n} {unit_fr}"),
        }
    }
}

pub(crate) static EN: Strings = Strings {
    lang: Lang::En,
    env_dev: "DEV",
    env_prod: "PROD",
    panel_nas: "NAS",
    panel_last: "Last backup",
    panel_actions: "Actions",
    panel_terminal: "Terminal",
    nas_online: "ONLINE",
    nas_offline: "OFFLINE",
    nas_checking: "checking…",
    nas_no_config: "no valid configuration (Configuration → check)",
    field_share: "Share",
    field_folder: "Folder",
    field_sources: "Sources",
    field_retention: "Retention",
    field_encryption: "Encryption",
    field_status: "Status",
    field_date: "Date",
    field_size: "Size",
    field_duration: "Duration",
    field_last_ok: "Last success",
    on: "on",
    off: "off",
    files: "files",
    no_history: "No backup recorded on this computer yet.",
    stale_warning: "last success is over 7 days old: time for a new backup",
    partial: "partial",
    status_completed: "Completed",
    status_with_errors: "Completed with errors",
    status_failed: "Failed",
    status_in_progress: "In progress (or interrupted)",
    terminal_empty: "Output of the operations appears here.",
    keys_idle: "↑↓ choose · Enter run · letter = shortcut · Tab/→ terminal · L language · q quit",
    keys_busy: "Ctrl-C interrupt · Tab switch panel",
    keys_prompt: "↑↓ choose · Enter confirm · Esc cancel",
    keys_terminal: "↑↓ scroll · PgUp/PgDn page · Home/End top/bottom · Tab/←/Esc menu",
    keep_all: "keep all",
    running: "Running:",
    busy_refused: "An operation is already running (Ctrl-C to interrupt it).",
    interrupt_requested: "Interruption requested: stopping the transfer…",
    result_ok: "Finished",
    result_warnings: "Finished with warnings",
    result_failed: "Failed",
    result_interrupted: "Interrupted",
    error: "Error",
    password_prompt: "SMB password for",
    password_rejected: "The NAS rejected the password: it will be asked again.",
    editor_running: "Editor open in the terminal…",
    menu_backup: "Start a backup",
    menu_status: "Status / history",
    menu_list: "Backups on the NAS",
    menu_verify: "Verify a backup",
    menu_restore: "Restore",
    menu_seal: "Seal sensitive sources",
    menu_config: "Configuration",
    menu_language: "Langue / Language",
    menu_quit: "Quit",
    language_saved: "Language switched to English (saved in the configuration file).",
    status_prompt: "What do you want to see?",
    status_options: [
        "Last backup summary",
        "Last backup (detailed)",
        "Full history",
        "Clear local history (not NAS backups)",
    ],
    history_clear_prompt: "Delete these local summaries? NAS backups are unchanged",
    config_prompt: "Configuration: what do you want to do?",
    config_options: [
        "Show the configuration",
        "Show the sources and exclusions",
        "Validate the configuration (check)",
        "Configuration file path",
        "Open in $EDITOR",
        "Configure a sealed source",
    ],
    yes: "Yes",
    no: "No",
    backup_source_prompt: "What to back up?",
    backup_source_options: [
        "Configured sources",
        "Specific files/folders",
        "Sealed archives only",
    ],
    backup_paths_prompt: "Paths to back up (comma separated, empty = configured sources)",
    backup_list_prompt: "List every file in the dry run?",
    backup_dryrun_prompt: "Run a dry run first (recommended)?",
    backup_run_prompt: "Start the real backup now?",
    seal_run_prompt: "Create the encrypted archives locally now (no NAS transfer)?",
    seal_action_prompt: "Sensitive sources: what do you want to do?",
    seal_options: [
        "Create encrypted archives locally",
        "Back up sealed archives only",
        "Add/update a sealed source",
        "Store/replace recovery passphrase in keyring",
    ],
    sealed_backup_prompt: "NAS deletion finished? Upload sealed archives only now?",
    seal_source_prompt: "Sensitive file/folder to add or update",
    seal_archive_prompt: "Unique archive filename (.tar.gz.age)",
    seal_staging_prompt: "Private staging directory (outside sensitive sources)",
    seal_save_prompt: "Save this sealed configuration? Other sources are preserved",
    seal_secret_warning: "Store/replace the keyring passphrase?\nOld archives need their old passphrase.\nKeep an independent offline copy.\nAn environment override still wins.",
    seal_password_prompt: "Recovery passphrase (keep an independent offline copy)",
    seal_password_confirm: "Confirm recovery passphrase",
    verify_prompt: "Which backup do you want to verify?",
    verify_latest: "Latest usable backup",
    verify_deep_prompt: "Deep verification (download every file and check its SHA-256; slow)?",
    no_backup_on_nas: "No backup found on the NAS.",
    restore_prompt: "Which backup do you want to restore?",
    restore_latest: "Latest usable backup",
    restore_target_prompt: "Target folder (empty = original locations)",
    restore_paths_prompt: "Only restore these paths (comma separated, empty = everything)",
    restore_overwrite_prompt: "Overwrite existing files that differ from the backup?",
    restore_run_prompt: "Run the restore now?",
};

pub(crate) static FR: Strings = Strings {
    lang: Lang::Fr,
    env_dev: "DEV",
    env_prod: "PROD",
    panel_nas: "NAS",
    panel_last: "Dernière sauvegarde",
    panel_actions: "Actions",
    panel_terminal: "Terminal",
    nas_online: "EN LIGNE",
    nas_offline: "HORS LIGNE",
    nas_checking: "vérification…",
    nas_no_config: "configuration invalide (Configuration → check)",
    field_share: "Partage",
    field_folder: "Dossier",
    field_sources: "Sources",
    field_retention: "Rétention",
    field_encryption: "Chiffrement",
    field_status: "Statut",
    field_date: "Date",
    field_size: "Taille",
    field_duration: "Durée",
    field_last_ok: "Dernier succès",
    on: "activé",
    off: "désactivé",
    files: "fichiers",
    no_history: "Aucune sauvegarde enregistrée sur cet ordinateur.",
    stale_warning: "dernier succès il y a plus de 7 jours : lancez une sauvegarde",
    partial: "partielle",
    status_completed: "Terminée",
    status_with_errors: "Terminée avec erreurs",
    status_failed: "Échec",
    status_in_progress: "En cours (ou interrompue)",
    terminal_empty: "La sortie des opérations s'affiche ici.",
    keys_idle: "↑↓ choisir · Entrée lancer · lettre = raccourci · Tab/→ terminal · L langue · q quitter",
    keys_busy: "Ctrl-C interrompre · Tab changer de panneau",
    keys_prompt: "↑↓ choisir · Entrée valider · Échap annuler",
    keys_terminal: "↑↓ défiler · PgPréc/PgSuiv page · Début/Fin haut/bas · Tab/←/Échap menu",
    keep_all: "tout garder",
    running: "En cours :",
    busy_refused: "Une opération est déjà en cours (Ctrl-C pour l'interrompre).",
    interrupt_requested: "Interruption demandée : arrêt du transfert…",
    result_ok: "Terminé",
    result_warnings: "Terminé avec avertissements",
    result_failed: "Échec",
    result_interrupted: "Interrompu",
    error: "Erreur",
    password_prompt: "Mot de passe SMB pour",
    password_rejected: "Le NAS a refusé le mot de passe : il sera redemandé.",
    editor_running: "Éditeur ouvert dans le terminal…",
    menu_backup: "Lancer une sauvegarde",
    menu_status: "Statut / historique",
    menu_list: "Sauvegardes sur le NAS",
    menu_verify: "Vérifier une sauvegarde",
    menu_restore: "Restaurer",
    menu_seal: "Sceller les sources",
    menu_config: "Configuration",
    menu_language: "Langue / Language",
    menu_quit: "Quitter",
    language_saved: "Langue changée : français (enregistrée dans le fichier de configuration).",
    status_prompt: "Que voulez-vous voir ?",
    status_options: [
        "Résumé de la dernière sauvegarde",
        "Dernière sauvegarde (détaillée)",
        "Historique complet",
        "Effacer l'historique local (pas les backups NAS)",
    ],
    history_clear_prompt: "Supprimer ces résumés locaux ? Backups NAS inchangés",
    config_prompt: "Configuration : que voulez-vous faire ?",
    config_options: [
        "Voir la configuration",
        "Voir les sources et exclusions",
        "Valider la configuration (check)",
        "Chemin du fichier de configuration",
        "Ouvrir dans $EDITOR",
        "Configurer une source scellée",
    ],
    yes: "Oui",
    no: "Non",
    backup_source_prompt: "Que sauvegarder ?",
    backup_source_options: [
        "Sources configurées",
        "Fichiers/dossiers précis",
        "Archives scellées uniquement",
    ],
    backup_paths_prompt: "Chemins à sauvegarder (séparés par des virgules, vide = sources configurées)",
    backup_list_prompt: "Lister tous les fichiers dans le dry-run ?",
    backup_dryrun_prompt: "Faire d'abord un dry-run (recommandé) ?",
    backup_run_prompt: "Lancer la vraie sauvegarde maintenant ?",
    seal_run_prompt: "Créer les archives chiffrées localement maintenant (sans transfert NAS) ?",
    seal_action_prompt: "Sources sensibles : que voulez-vous faire ?",
    seal_options: [
        "Créer les archives chiffrées localement",
        "Sauvegarder uniquement les archives scellées",
        "Ajouter/modifier une source scellée",
        "Enregistrer/remplacer la passphrase dans le trousseau",
    ],
    sealed_backup_prompt: "Suppression NAS terminée ? Envoyer uniquement les archives scellées ?",
    seal_source_prompt: "Fichier/dossier sensible à ajouter ou modifier",
    seal_archive_prompt: "Nom d'archive unique (.tar.gz.age)",
    seal_staging_prompt: "Dossier de staging privé (hors des sources sensibles)",
    seal_save_prompt: "Enregistrer cette configuration ? Autres sources conservées",
    seal_secret_warning: "Enregistrer/remplacer la passphrase ?\nLes anciens backups gardent leur passphrase.\nGardez une copie indépendante hors ligne.\nLa variable d'environnement est prioritaire.",
    seal_password_prompt: "Passphrase de récupération (gardez une copie indépendante hors ligne)",
    seal_password_confirm: "Confirmer la passphrase de récupération",
    verify_prompt: "Quelle sauvegarde voulez-vous vérifier ?",
    verify_latest: "Dernière sauvegarde utilisable",
    verify_deep_prompt: "Vérification approfondie (télécharge chaque fichier et vérifie le SHA-256 ; lent) ?",
    no_backup_on_nas: "Aucune sauvegarde trouvée sur le NAS.",
    restore_prompt: "Quelle sauvegarde voulez-vous restaurer ?",
    restore_latest: "Dernière sauvegarde utilisable",
    restore_target_prompt: "Dossier cible (vide = emplacements d'origine)",
    restore_paths_prompt: "Restaurer seulement ces chemins (séparés par des virgules, vide = tout)",
    restore_overwrite_prompt: "Écraser les fichiers existants différents de la sauvegarde ?",
    restore_run_prompt: "Lancer la restauration maintenant ?",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_roundtrip_and_toggle() {
        assert_eq!(Lang::from_config("en"), Lang::En);
        assert_eq!(Lang::from_config("fr"), Lang::Fr);
        assert_eq!(Lang::from_config("de"), Lang::En); // invalid falls back to English
        assert_eq!(Lang::En.toggle(), Lang::Fr);
        assert_eq!(Lang::Fr.toggle(), Lang::En);
        assert_eq!(Lang::from_config(Lang::Fr.as_str()), Lang::Fr);
    }

    #[test]
    fn both_languages_define_the_menu() {
        for lang in [Lang::En, Lang::Fr] {
            let s = lang.strings();
            assert_eq!(s.lang, lang);
            assert_eq!(s.status_options.len(), 4);
            assert_eq!(s.config_options.len(), 6);
            assert_eq!(s.backup_source_options.len(), 3);
            assert_eq!(s.seal_options.len(), 4);
            assert_ne!(s.yes, s.no);
        }
        assert_ne!(Lang::En.strings().menu_quit, Lang::Fr.strings().menu_quit);
        assert_ne!(
            Lang::En.strings().menu_backup,
            Lang::Fr.strings().menu_backup
        );
    }

    #[test]
    fn localized_ages() {
        let (en, fr) = (Lang::En.strings(), Lang::Fr.strings());
        assert_eq!(en.age(10), "just now");
        assert_eq!(fr.age(10), "à l'instant");
        assert_eq!(en.age(300), "5 min ago");
        assert_eq!(fr.age(7200), "il y a 2 h");
        assert_eq!(fr.age(3 * 86400), "il y a 3 jours");
        assert_eq!(en.age(-5), "just now");
    }

    #[test]
    fn localized_statuses() {
        assert_eq!(Lang::Fr.strings().status(BackupStatus::Failed), "Échec");
        assert_eq!(
            Lang::En.strings().status(BackupStatus::Completed),
            "Completed"
        );
    }
}
