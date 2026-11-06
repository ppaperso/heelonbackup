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
    pub stub: &'static str,
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
    stub: "This flow is not implemented yet (phase 3).",
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
    stub: "Ce flux sera implémenté en phase 3.",
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
            assert!(!s.stub.is_empty());
        }
        assert_ne!(Lang::En.strings().menu_quit, Lang::Fr.strings().menu_quit);
        assert_ne!(
            Lang::En.strings().menu_backup,
            Lang::Fr.strings().menu_backup
        );
    }
}
