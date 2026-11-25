//! Client HTTP réel : HTTPS seulement, certificats vérifiés par le magasin de
//! Windows (aucune exception), chaque redirection contrôlée par la liste des
//! domaines autorisés.

use crate::catalog::{HttpGet, HttpResponse};
use crate::security::domains;
use reqwest::blocking::Client;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::redirect::Policy;
use reqwest::StatusCode;
use std::io::Read;
use std::time::Duration;

/// Taille maximale d'un catalogue (protection contre une réponse démesurée).
const MAX_CATALOG_BYTES: u64 = 5 * 1024 * 1024;
const MAX_REDIRECTS: usize = 10;

pub fn user_agent() -> String {
    format!("BworkshopLauncher/{}", env!("CARGO_PKG_VERSION"))
}

/// Politique de redirection commune : chaque saut doit être autorisé.
pub fn redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("trop de redirections");
        }
        let check = match attempt.previous().last() {
            Some(from) => domains::check_redirect(from, attempt.url()),
            None => domains::check_initial(attempt.url()),
        };
        match check {
            Ok(()) => attempt.follow(),
            Err(e) => {
                log::warn!("Redirection refusée : {e}");
                attempt.error(e)
            }
        }
    })
}

pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .https_only(true)
            .redirect(redirect_policy())
            .user_agent(user_agent())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| format!("client HTTP : {e}"))?;
        Ok(Self { client })
    }
}

impl HttpGet for HttpClient {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<HttpResponse, String> {
        let parsed = domains::check_initial_str(url).map_err(|e| {
            log::warn!("Adresse refusée : {e}");
            e.to_string()
        })?;
        let mut request = self.client.get(parsed);
        if let Some(tag) = etag {
            request = request.header(IF_NONE_MATCH, tag);
        }
        let response = request.send().map_err(|e| format!("réseau : {e}"))?;
        match response.status() {
            StatusCode::NOT_MODIFIED => Ok(HttpResponse::NotModified),
            s if s.is_success() => {
                let etag = response
                    .headers()
                    .get(ETAG)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                let mut body = Vec::new();
                response
                    .take(MAX_CATALOG_BYTES + 1)
                    .read_to_end(&mut body)
                    .map_err(|e| format!("lecture : {e}"))?;
                if body.len() as u64 > MAX_CATALOG_BYTES {
                    return Err("réponse trop volumineuse".into());
                }
                Ok(HttpResponse::Ok { body, etag })
            }
            s => Err(format!("réponse HTTP {s}")),
        }
    }
}
