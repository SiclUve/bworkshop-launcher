//! Liste des domaines autorisés pour les téléchargements et les catalogues.
//!
//! - Autorisés : `bworkshop.app`, `github.com/SiclUve/…`, `github.com/rubiconte/…`.
//! - `objects.githubusercontent.com` et `release-assets.githubusercontent.com` :
//!   acceptés **seulement** comme destination d'une redirection depuis un lien autorisé.
//! - HTTPS obligatoire, port par défaut seulement, pas d'identifiants dans l'adresse.
//! - Tout le reste est refusé ; l'appelant journalise le refus.

use url::Url;

pub const ALLOWED_HOSTS: &[&str] = &["bworkshop.app"];
pub const ALLOWED_GITHUB_OWNERS: &[&str] = &["SiclUve", "rubiconte"];
pub const REDIRECT_ONLY_HOSTS: &[&str] = &[
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("adresse invalide")]
    Invalid,
    #[error("adresse non HTTPS refusée : {0}")]
    NotHttps(String),
    #[error("adresse avec port ou identifiants refusée : {0}")]
    Unusual(String),
    #[error("adresse hors de la liste autorisée : {0}")]
    NotAllowed(String),
    #[error("redirection vers un hôte non autorisé : {0}")]
    RedirectNotAllowed(String),
}

/// Forme courte d'une adresse pour les messages : hôte + chemin, sans paramètres
/// (les liens signés de GitHub ont de longs paramètres sans intérêt).
pub fn short(url: &Url) -> String {
    format!("{}{}", url.host_str().unwrap_or("?"), url.path())
}

fn base_checks(url: &Url) -> Result<String, DomainError> {
    if url.scheme() != "https" {
        return Err(DomainError::NotHttps(short(url)));
    }
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return Err(DomainError::Unusual(short(url)));
    }
    url.host_str()
        .map(|h| h.to_ascii_lowercase())
        .ok_or(DomainError::Invalid)
}

/// Vérifie une adresse de départ (catalogue, lien de téléchargement).
pub fn check_initial(url: &Url) -> Result<(), DomainError> {
    let host = base_checks(url)?;
    if ALLOWED_HOSTS.contains(&host.as_str()) {
        return Ok(());
    }
    if host == "github.com" {
        let owner = url.path_segments().and_then(|mut s| s.next()).unwrap_or("");
        if ALLOWED_GITHUB_OWNERS.iter().any(|o| o.eq_ignore_ascii_case(owner)) {
            return Ok(());
        }
    }
    Err(DomainError::NotAllowed(short(url)))
}

/// Vérifie une redirection de `from` vers `to`.
pub fn check_redirect(from: &Url, to: &Url) -> Result<(), DomainError> {
    let host = base_checks(to)?;
    if check_initial(to).is_ok() {
        return Ok(());
    }
    if REDIRECT_ONLY_HOSTS.contains(&host.as_str()) && check_initial(from).is_ok() {
        return Ok(());
    }
    Err(DomainError::RedirectNotAllowed(short(to)))
}

/// Raccourci : analyse puis vérifie une adresse de départ.
pub fn check_initial_str(url: &str) -> Result<Url, DomainError> {
    let parsed = Url::parse(url).map_err(|_| DomainError::Invalid)?;
    check_initial(&parsed)?;
    Ok(parsed)
}

/// Navigation dans un widget : seulement le domaine de l'outil, en HTTPS
/// (avec ou sans `www.`), plus `about:blank`. Le reste part dans le navigateur.
pub fn widget_navigation_allowed(url: &Url, tool_host: &str) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() {
        return false;
    }
    let bare = |h: &str| h.strip_prefix("www.").unwrap_or(h).to_ascii_lowercase();
    url.host_str().is_some_and(|h| bare(h) == bare(tool_host))
}

#[cfg(test)]
mod widget_tests {
    use super::*;

    #[test]
    fn navigation_des_widgets() {
        let ok = |s: &str| widget_navigation_allowed(&Url::parse(s).unwrap(), "bworkshop.app");
        assert!(ok("https://bworkshop.app/forge-app/"));
        assert!(ok("https://www.bworkshop.app/autre"));
        assert!(ok("about:blank"));
        assert!(!ok("http://bworkshop.app/"));
        assert!(!ok("https://bworkshop.app.evil.com/"));
        assert!(!ok("https://evil.com/?bworkshop.app"));
        assert!(!ok("https://user@bworkshop.app/"));
        assert!(!ok("https://bworkshop.app:8443/"));
        assert!(!ok("javascript:alert(1)"));
        assert!(!ok("file:///C:/Windows/"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn adresses_autorisees() {
        assert!(check_initial(&u("https://bworkshop.app/catalog.json")).is_ok());
        assert!(check_initial(&u("https://BWORKSHOP.app/downloads/x.exe")).is_ok());
        assert!(check_initial(&u(
            "https://github.com/SiclUve/bworkshop-releases/releases/download/v0.2.0/a.exe"
        ))
        .is_ok());
        assert!(check_initial(&u("https://github.com/rubiconte/GrimoireBoss/releases/download/x/a.exe")).is_ok());
        // Les noms GitHub ne tiennent pas compte de la casse.
        assert!(check_initial(&u("https://github.com/sicluve/x")).is_ok());
    }

    #[test]
    fn adresses_hors_liste_refusees() {
        for bad in [
            "https://evil.example/a.exe",
            "https://github.com/autre/projet/releases/a.exe",
            "https://github.com/",
            "https://bworkshop.app.evil.example/a.exe",
            "https://sub.bworkshop.app/a.exe",
            "https://release-assets.githubusercontent.com/a", // jamais en lien direct
            "https://objects.githubusercontent.com/a",
        ] {
            assert!(matches!(check_initial(&u(bad)), Err(DomainError::NotAllowed(_))), "{bad}");
        }
    }

    #[test]
    fn https_obligatoire_et_adresses_inhabituelles() {
        assert!(matches!(check_initial(&u("http://bworkshop.app/a")), Err(DomainError::NotHttps(_))));
        assert!(matches!(check_initial(&u("https://bworkshop.app:8443/a")), Err(DomainError::Unusual(_))));
        assert!(matches!(check_initial(&u("https://moi@bworkshop.app/a")), Err(DomainError::Unusual(_))));
        // Le port 443 explicite est normalisé par l'analyseur : accepté.
        assert!(check_initial(&u("https://bworkshop.app:443/a")).is_ok());
    }

    #[test]
    fn redirection_github_acceptee() {
        let from = u("https://github.com/SiclUve/bworkshop-releases/releases/download/v0.2.0/a.exe");
        let to = u("https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sp=r");
        assert!(check_redirect(&from, &to).is_ok());
        let to = u("https://objects.githubusercontent.com/x");
        assert!(check_redirect(&from, &to).is_ok());
    }

    #[test]
    fn redirection_vers_un_autre_hote_refusee() {
        let from = u("https://github.com/SiclUve/x/releases/download/v1/a.exe");
        assert!(matches!(
            check_redirect(&from, &u("https://evil.example/a.exe")),
            Err(DomainError::RedirectNotAllowed(_))
        ));
        assert!(matches!(
            check_redirect(&from, &u("http://release-assets.githubusercontent.com/a")),
            Err(DomainError::NotHttps(_))
        ));
        // Hôte « redirection seulement » atteint depuis un lien non autorisé : refusé.
        let from = u("https://github.com/autre/x");
        assert!(check_redirect(&from, &u("https://release-assets.githubusercontent.com/a")).is_err());
    }

    #[test]
    fn message_sans_parametres() {
        let e = check_initial(&u("https://evil.example/a.exe?token=secret")).unwrap_err();
        assert!(!e.to_string().contains("secret"));
    }
}
