//! Mise à jour du launcher lui-même (tauri-plugin-updater), **signée**.
//!
//! - Le fichier `latest.json` et l'installateur sont publiés sur
//!   https://github.com/SiclUve/bworkshop-releases/releases (domaine autorisé). `latest.json`
//!   est toujours dans la release `launcher-latest` : la « dernière release » du dépôt peut
//!   être celle d'un autre outil.
//! - Chaque installateur est signé avec une clé dédiée. La clé **privée** reste hors du dépôt
//!   (sur la machine de celui qui publie) ; seule la clé publique est embarquée
//!   (`src-tauri/updater-key.pub`). Une signature invalide : la mise à jour est refusée.
//! - La version signée doit être celle annoncée (`requireSignedVersion`) : impossible de
//!   faire réinstaller une vieille version en trafiquant `latest.json`.
//! - Jamais en pleine partie : la vérification est silencieuse, rien ne surgit. La mise à
//!   jour est seulement **proposée** dans la fenêtre du launcher, et ne s'installe que sur
//!   un clic de l'utilisateur.
//! - Sans clé publique (fichier vide), la mise à jour automatique est désactivée.

use bworkshop_core::security::domains;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_updater::UpdaterExt;

const ENDPOINT: &str = "https://github.com/SiclUve/bworkshop-releases/releases/download/launcher-latest/latest.json";
const PUBKEY: &str = include_str!("../updater-key.pub");
/// Première vérification après le démarrage, puis toutes les 6 heures.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(6 * 3600);

/// Événements envoyés à la fenêtre principale.
pub const EVENT_AVAILABLE: &str = "launcher-update";
pub const EVENT_PROGRESS: &str = "launcher-update-progress";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdate {
    pub version: String,
    pub notes: Option<String>,
}

#[derive(Default)]
pub struct UpdaterState {
    available: Mutex<Option<LauncherUpdate>>,
    installing: AtomicBool,
}

fn configured() -> bool {
    !PUBKEY.trim().is_empty()
}

/// Le module à brancher au démarrage, seulement si une clé publique est embarquée.
/// La clé vient de `updater-key.pub`. Le champ `"pubkey": ""` de `tauri.conf.json` doit
/// rester (vide) : sans lui, la configuration du module est refusée et le launcher ne démarre pas.
pub fn plugin() -> Option<tauri::plugin::TauriPlugin<Wry, tauri_plugin_updater::Config>> {
    configured().then(|| tauri_plugin_updater::Builder::new().pubkey(PUBKEY.trim()).build())
}

/// Vérifications en arrière-plan (silencieuses).
pub fn start(app: &AppHandle) {
    if !configured() {
        log::info!("Mise à jour du launcher désactivée : aucune clé publique embarquée");
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(FIRST_CHECK).await;
        loop {
            if let Err(e) = check(&app).await {
                log::warn!("Vérification de mise à jour du launcher impossible : {e}");
            }
            tokio_sleep(EVERY).await;
        }
    });
}

/// Attente entre deux vérifications. Elle occupe un fil du réservoir « bloquant » de Tauri
/// pendant toute sa durée (jusqu'à 6 h) : coût négligeable, sans dépendance directe à tokio.
async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let url = ENDPOINT.parse().map_err(|e| format!("{e}"))?;
    let app_exit = app.clone();
    app.updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .timeout(Duration::from_secs(30))
        // L'installateur ferme le launcher sans passer par la sortie normale : on enregistre
        // d'abord ce que la sortie normale aurait enregistré.
        .on_before_exit(move || {
            app_exit.state::<crate::widgets::Widgets>().flush(&app_exit);
            log::info!("Fermeture pour installer la mise à jour");
            log::logger().flush();
        })
        .build()
        .map_err(|e| e.to_string())
}

/// Cherche une nouvelle version ; la mémorise et prévient la fenêtre principale.
async fn check(app: &AppHandle) -> Result<(), String> {
    let found = updater(app)?.check().await.map_err(|e| e.to_string())?;
    let Some(update) = found else {
        log::info!("Launcher à jour");
        return Ok(());
    };
    // L'installateur doit venir d'un domaine autorisé, comme tout téléchargement.
    if let Err(e) = domains::check_initial(&update.download_url) {
        log::warn!("Mise à jour du launcher ignorée : adresse refusée ({e})");
        return Ok(());
    }
    let info = LauncherUpdate { version: update.version.clone(), notes: update.body.clone() };
    log::info!("Mise à jour du launcher disponible : {}", info.version);
    if let Ok(mut a) = app.state::<UpdaterState>().available.lock() {
        *a = Some(info.clone());
    }
    let _ = app.emit_to(crate::state::MAIN_LABEL, EVENT_AVAILABLE, info);
    Ok(())
}

/// Télécharge, vérifie la signature et lance l'installateur (le launcher se ferme puis
/// redémarre à jour). Seulement sur un clic de l'utilisateur.
async fn install(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<UpdaterState>();
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err("Mise à jour déjà en cours.".into());
    }
    let result = async {
        let update = updater(app)?.check().await.map_err(|e| e.to_string())?.ok_or("Le launcher est déjà à jour.")?;
        domains::check_initial(&update.download_url).map_err(|e| format!("Adresse refusée : {e}"))?;
        log::info!("Installation de la mise à jour du launcher {}", update.version);
        let mut received = 0usize;
        let app2 = app.clone();
        update
            .download_and_install(
                move |chunk, total| {
                    received += chunk;
                    let percent = total.filter(|t| *t > 0).map(|t| (received as u64 * 100 / t).min(100));
                    let _ = app2.emit_to(crate::state::MAIN_LABEL, EVENT_PROGRESS, percent);
                },
                || log::info!("Mise à jour du launcher téléchargée, signature vérifiée"),
            )
            .await
            .map_err(|e| format!("Mise à jour refusée : {e}"))
    }
    .await;
    state.installing.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
pub fn get_launcher_update(app: AppHandle) -> Option<LauncherUpdate> {
    app.state::<UpdaterState>().available.lock().ok().and_then(|a| a.clone())
}

#[tauri::command]
pub async fn install_launcher_update(app: AppHandle) -> Result<(), String> {
    if !configured() {
        return Err("La mise à jour automatique n'est pas configurée.".into());
    }
    install(&app).await
}
