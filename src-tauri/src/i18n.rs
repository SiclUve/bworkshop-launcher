//! Textes du menu de la zone de notification (le reste est traduit côté interface).

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
        (Language::Fr, "quit") => "Quitter",
        (Language::En, "quit") => "Quit",
        _ => "?",
    }
}
