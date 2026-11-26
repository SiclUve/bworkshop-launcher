//! Textes affichés par Rust : menus natifs (zone de notification, barre d'outils).
//! Le reste est traduit côté interface (`ui/src/shared/i18n.ts`).

use bworkshop_core::settings::Language;

pub fn t(lang: Language, key: &str) -> &'static str {
    match (lang, key) {
        (Language::Fr, "open") => "Ouvrir",
        (Language::En, "open") => "Open",
        (Language::Fr, "layouts") => "Dispositions",
        (Language::En, "layouts") => "Layouts",
        (Language::Fr, "no_layout") => "Aucune disposition",
        (Language::En, "no_layout") => "No layout",
        (Language::Fr, "hide_all") => "Tout cacher",
        (Language::En, "hide_all") => "Hide all",
        (Language::Fr, "show_all") => "Tout montrer",
        (Language::En, "show_all") => "Show all",
        (Language::Fr, "quit") => "Quitter",
        (Language::En, "quit") => "Quit",
        _ => "?",
    }
}

/// Texte écrit sur place dans les deux langues (menus de la barre d'outils).
pub fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}
