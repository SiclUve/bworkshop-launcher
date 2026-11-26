//! Versions web des outils.
//!
//! Adresses de repli quand le catalogue ne fournit pas de champ `webUrl` (outils
//! officiels) ou `overlayUrl` ; un champ présent dans le catalogue est toujours
//! prioritaire (voir `parse.rs`).

/// Repli : identifiant officiel → version web.
pub const OFFICIAL_WEB_URLS: &[(&str, &str)] = &[
    ("assistant-forge", "https://bworkshop.app/forge-app/"),
    ("simulateur-harebourg", "https://bworkshop.app/harebourg-app/"),
    ("bworklingo", "https://bworkshop.app/bworklingo-app/"),
    ("skin-helper", "https://bworkshop.app/skinhelper-app/"),
];

/// Mode « overlay » (compact, pensé pour flotter au-dessus du jeu) : paramètre ajouté
/// à la version web quand le catalogue ne fournit pas de champ `overlayUrl`.
/// - forge / harebourg : `?overlay=1` (le panneau de la fenêtre « Détacher en Overlay ») ;
/// - bworklingo : `?overlay_only=true` ;
/// - grimoire-boss : `?widget=1`.
pub const OVERLAY_QUERIES: &[(&str, &str)] = &[
    ("assistant-forge", "overlay=1"),
    ("simulateur-harebourg", "overlay=1"),
    ("bworklingo", "overlay_only=true"),
    ("grimoire-boss", "widget=1"),
];

pub fn official_fallback(id: &str) -> Option<&'static str> {
    OFFICIAL_WEB_URLS.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

/// Ajoute un paramètre `clé=valeur` à une adresse, s'il n'y est pas déjà
/// (comparaison par paramètre : `overlay=10` n'est pas `overlay=1`).
fn with_query(url: &str, q: &str) -> String {
    let (key, value) = q.split_once('=').unwrap_or((q, ""));
    let present = url::Url::parse(url).is_ok_and(|u| u.query_pairs().any(|(k, v)| k == key && v == value));
    if present {
        return url.to_string();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{q}")
}

/// Version overlay déduite de la version web, si l'outil en a une.
pub fn overlay_for(id: &str, web_url: &str) -> Option<String> {
    OVERLAY_QUERIES.iter().find(|(k, _)| *k == id).map(|(_, q)| with_query(web_url, q))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_overlay() {
        assert_eq!(
            overlay_for("grimoire-boss", "https://bworkshop.app/tacle-retrait-app/").as_deref(),
            Some("https://bworkshop.app/tacle-retrait-app/?widget=1")
        );
        assert_eq!(overlay_for("assistant-forge", "https://x/a?lang=en").as_deref(), Some("https://x/a?lang=en&overlay=1"));
        assert_eq!(overlay_for("bworklingo", "https://x/?overlay_only=true").as_deref(), Some("https://x/?overlay_only=true"));
        assert_eq!(overlay_for("skin-helper", "https://x/"), None);
        // Paramètre voisin mais différent : le bon est ajouté.
        assert_eq!(overlay_for("assistant-forge", "https://x/?overlay=10").as_deref(), Some("https://x/?overlay=10&overlay=1"));
    }
}
