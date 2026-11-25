//! Modèle unifié d'un outil, après validation. C'est ce que reçoit l'interface.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Official,
    Community,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    #[serde(rename = "beta")]
    Beta,
    #[serde(rename = "stable")]
    Stable,
    #[serde(rename = "en-developpement")]
    InDevelopment,
}

/// Texte en français et en anglais.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Localized {
    #[serde(default)]
    pub fr: String,
    #[serde(default)]
    pub en: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub url: String,
    /// Taille annoncée (absente pour les plugins communautaires).
    pub size_bytes: Option<u64>,
    /// Empreinte SHA-256 en minuscules (peut manquer : confirmation demandée).
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: String,
    pub source: Source,
    pub name: String,
    pub author: String,
    pub category: String,
    pub kinds: Vec<String>,
    pub platforms: Vec<String>,
    pub status: Status,
    /// Version semver validée.
    pub version: Option<String>,
    pub order: i64,
    pub tagline: Option<Localized>,
    pub description: Option<Localized>,
    pub download: Option<Download>,
    /// Version web, ouvrable en widget.
    pub web_url: Option<String>,
    /// Version overlay (compacte, au-dessus du jeu), si l'outil en a une.
    pub overlay_url: Option<String>,
    /// Icône carrée de l'outil (PNG, WebP ou JPEG, sur un domaine autorisé).
    pub icon_url: Option<String>,
    pub page_url: Option<String>,
    pub source_url: Option<String>,
    pub virustotal_url: Option<String>,
    pub updated_at: Option<String>,
    /// Téléchargeable maintenant (sinon affiché « Bientôt »).
    pub available: bool,
}

impl Tool {
    pub fn parsed_version(&self) -> Option<semver::Version> {
        self.version.as_deref().and_then(|v| semver::Version::parse(v).ok())
    }

    pub fn has_kind(&self, kind: &str) -> bool {
        self.kinds.iter().any(|k| k == kind)
    }

    /// Toutes les adresses de liens affichables (pour n'ouvrir que celles-ci).
    pub fn links(&self) -> impl Iterator<Item = &str> {
        [&self.page_url, &self.source_url, &self.virustotal_url]
            .into_iter()
            .filter_map(|u| u.as_deref())
    }
}
