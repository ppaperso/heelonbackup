//! Bilingual (EN/FR) labels of the interactive console
//!
//! The rest of the application (CLI, errors, logs) stays in English; only
//! the console display is bilingual. The active language is stored in the
//! configuration file (`language`: "en" | "fr", default "en") and can be
//! toggled from the main menu (« Langue / Language »).
//!
//! Each guided flow adds its own labels here in its dedicated commit.

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

/// Console labels, in one language (flow labels are added with their flow)
pub(crate) struct Strings {
    pub menu_prompt: &'static str,
    pub menu_backup: &'static str,
    pub menu_status: &'static str,
    pub menu_verify: &'static str,
    pub menu_restore: &'static str,
    pub menu_config: &'static str,
    pub menu_language: &'static str,
    pub menu_quit: &'static str,
    pub language_saved: &'static str,
    pub status_prompt: &'static str,
    pub status_options: [&'static str; 3],
    pub config_prompt: &'static str,
    pub config_options: [&'static str; 3],
    pub yes: &'static str,
    pub no: &'static str,
    pub backup_source_prompt: &'static str,
    pub backup_source_options: [&'static str; 2],
    pub backup_paths_prompt: &'static str,
    pub backup_list_prompt: &'static str,
    pub backup_dryrun_prompt: &'static str,
    pub backup_run_prompt: &'static str,
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

pub(crate) static EN: Strings = Strings {
    menu_prompt: "HeelonBackup — what would you like to do?",
    menu_backup: "Start a backup",
    menu_status: "Status",
    menu_verify: "Verify a backup",
    menu_restore: "Restore",
    menu_config: "Configuration",
    menu_language: "Langue / Language",
    menu_quit: "Quit",
    language_saved: "Language switched to English (saved in the configuration file).",
    status_prompt: "What do you want to see?",
    status_options: [
        "Last backup summary",
        "Last backup (detailed)",
        "Full history",
    ],
    config_prompt: "Configuration: what do you want to do?",
    config_options: [
        "Show the configuration",
        "Configuration file path",
        "Open in $EDITOR",
    ],
    yes: "Yes",
    no: "No",
    backup_source_prompt: "What to back up?",
    backup_source_options: ["Configured sources", "Specific files/folders"],
    backup_paths_prompt: "Paths to back up (comma separated, empty = configured sources)",
    backup_list_prompt: "List every file in the dry run?",
    backup_dryrun_prompt: "Run a dry run first (recommended)?",
    backup_run_prompt: "Start the real backup now?",
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
    menu_prompt: "HeelonBackup — que souhaitez-vous faire ?",
    menu_backup: "Lancer un backup",
    menu_status: "Statut",
    menu_verify: "Vérifier un backup",
    menu_restore: "Restaurer",
    menu_config: "Configuration",
    menu_language: "Langue / Language",
    menu_quit: "Quitter",
    language_saved: "Langue changée : français (enregistrée dans le fichier de configuration).",
    status_prompt: "Que voulez-vous voir ?",
    status_options: [
        "Résumé du dernier backup",
        "Dernier backup (détaillé)",
        "Historique complet",
    ],
    config_prompt: "Configuration : que voulez-vous faire ?",
    config_options: [
        "Voir la configuration",
        "Chemin du fichier de configuration",
        "Ouvrir dans $EDITOR",
    ],
    yes: "Oui",
    no: "Non",
    backup_source_prompt: "Que sauvegarder ?",
    backup_source_options: ["Sources configurées", "Fichiers/dossiers précis"],
    backup_paths_prompt: "Chemins à sauvegarder (séparés par des virgules, vide = sources configurées)",
    backup_list_prompt: "Lister tous les fichiers dans le dry-run ?",
    backup_dryrun_prompt: "Faire d'abord un dry-run (recommandé) ?",
    backup_run_prompt: "Lancer le vrai backup maintenant ?",
    verify_prompt: "Quel backup voulez-vous vérifier ?",
    verify_latest: "Dernier backup utilisable",
    verify_deep_prompt: "Vérification approfondie (télécharge chaque fichier et vérifie le SHA-256 ; lent) ?",
    no_backup_on_nas: "Aucun backup trouvé sur le NAS.",
    restore_prompt: "Quel backup voulez-vous restaurer ?",
    restore_latest: "Dernier backup utilisable",
    restore_target_prompt: "Dossier cible (vide = emplacements d'origine)",
    restore_paths_prompt: "Restaurer seulement ces chemins (séparés par des virgules, vide = tout)",
    restore_overwrite_prompt: "Écraser les fichiers existants différents du backup ?",
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
        for s in [Lang::En.strings(), Lang::Fr.strings()] {
            assert!(!s.menu_prompt.is_empty());
            assert_eq!(s.status_options.len(), 3);
            assert_eq!(s.config_options.len(), 3);
            assert_eq!(s.backup_source_options.len(), 2);
            assert_ne!(s.yes, s.no);
        }
        assert_ne!(Lang::En.strings().menu_quit, Lang::Fr.strings().menu_quit);
        assert_ne!(
            Lang::En.strings().menu_backup,
            Lang::Fr.strings().menu_backup
        );
    }
}
