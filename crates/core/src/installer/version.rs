//! Comparaison de versions en semver (jamais en texte : « 0.10.0 » > « 0.9.0 »).

use semver::Version;

/// Lit une version de façon tolérante : « v1.2 » → 1.2.0, « 1.2.3.4 » → 1.2.3.
pub fn parse_lenient(raw: &str) -> Option<Version> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    if let Ok(v) = Version::parse(s) {
        return Some(v);
    }
    let parts: Vec<u64> = s
        .split('.')
        .take(3)
        .map(|p| p.trim().parse::<u64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if parts.is_empty() {
        return None;
    }
    let get = |i: usize| parts.get(i).copied().unwrap_or(0);
    Some(Version::new(get(0), get(1), get(2)))
}

/// `available` est-elle strictement plus récente que `installed` ?
/// Si l'une des deux est illisible, on ne propose pas de mise à jour.
pub fn is_newer(available: &str, installed: &str) -> bool {
    match (parse_lenient(available), parse_lenient(installed)) {
        (Some(a), Some(i)) => a > i,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparaison_semver_et_non_textuelle() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("1.0.0", "1.0.0-beta.2"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
    }

    #[test]
    fn lecture_tolerante() {
        assert_eq!(parse_lenient("v1.2"), Some(Version::new(1, 2, 0)));
        assert_eq!(parse_lenient("1.2.3.4"), Some(Version::new(1, 2, 3)));
        assert_eq!(parse_lenient(" 3 "), Some(Version::new(3, 0, 0)));
        assert_eq!(parse_lenient("abc"), None);
        assert!(!is_newer("abc", "1.0.0"));
    }
}
