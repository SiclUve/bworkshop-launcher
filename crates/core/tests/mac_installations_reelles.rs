//! Test réel (Mac seulement, lancé à la demande par le workflow « Tester les installations Mac ») :
//! prend le **vrai catalogue** du site, puis, pour chaque outil qui a une version Mac pour la
//! puce de la machine, fait exactement ce que fait le launcher :
//! téléchargement vérifié → installation du .dmg → outil reconnu → démarrage → désinstallation.
//!
//! Lancement : `cargo test -p bworkshop-core --test mac_installations_reelles -- --ignored --nocapture`
//! Variable facultative `BW_OUTILS` : identifiants séparés par des virgules (sinon tous).
#![cfg(target_os = "macos")]

use bworkshop_core::catalog::parse::{parse_community, parse_official};
use bworkshop_core::catalog::{merge, Tool, COMMUNITY_CATALOG_URL, OFFICIAL_CATALOG_URL};
use bworkshop_core::installer::download::{download, download_client, file_name_for, open_verified, DownloadRequest};
use bworkshop_core::installer::launch::{main_exe, spawn_app};
use bworkshop_core::installer::mac::{bundle_id_for, install_dmg, remove_app, MacApps};
use bworkshop_core::installer::registry::RegistrySource;
use bworkshop_core::installer::status::{entry_for, status_of, InstallStatus};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Temps pendant lequel l'outil doit rester ouvert pour être considéré comme « démarré ».
const TENUE: Duration = Duration::from_secs(10);

fn catalogue() -> Vec<Tool> {
    let client = download_client().expect("client");
    let get = |url: &str| client.get(url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).expect(url);
    let official = parse_official(&get(OFFICIAL_CATALOG_URL)).expect("catalogue officiel lisible");
    let community = parse_community(&get(COMMUNITY_CATALOG_URL)).expect("catalogue communautaire lisible");
    merge(official.tools, community.tools)
}

fn vivant(pid: u32) -> bool {
    let out = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().expect("ps");
    let stat = String::from_utf8_lossy(&out.stdout);
    let stat = stat.trim();
    !stat.is_empty() && !stat.starts_with('Z')
}

fn arreter(pid: u32) {
    // L'outil est chef de son groupe de processus : on arrête tout le groupe.
    let _ = Command::new("kill").args(["-TERM", &format!("-{pid}")]).status();
    std::thread::sleep(Duration::from_secs(2));
    let _ = Command::new("kill").args(["-KILL", &format!("-{pid}")]).status();
}

/// Un outil, de bout en bout. Rend `Ok(résumé)` ou `Err(étape : erreur)`.
fn essayer(tool: &Tool, work: &Path) -> Result<String, String> {
    let dl = tool.download.as_ref().ok_or("pas de version Mac")?;
    let sha = dl.sha256.as_deref().ok_or("pas d'empreinte SHA-256")?;
    let apps = work.join("Applications");
    let tmp = work.join("tmp");
    std::fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    // 1. Téléchargement vérifié (même code et mêmes contrôles que le launcher).
    let req = DownloadRequest {
        url: dl.url.clone(),
        dest: tmp.join(file_name_for(&dl.url, &tool.id)),
        expected_sha256: sha.to_string(),
        max_bytes: 2 * 1024 * 1024 * 1024,
    };
    let client = download_client().map_err(|e| format!("téléchargement : {e}"))?;
    let got = download(&client, &req, &AtomicBool::new(false), |_| {}).map_err(|e| format!("téléchargement : {e}"))?;
    let _verrou = open_verified(&got.path, sha).map_err(|e| format!("vérification : {e}"))?;

    // 2. Installation.
    let app = install_dmg(&got.path, &apps, &tmp, bundle_id_for(&tool.id)).map_err(|e| format!("installation : {e}"))?;

    // 3. Reconnu par le launcher, à la bonne version.
    let entries = MacApps { dir: apps.clone() }.entries();
    let entry = entry_for(&entries, tool).ok_or_else(|| {
        let noms: Vec<_> = entries.iter().map(|e| format!("{} ({})", e.display_name.as_deref().unwrap_or("?"), e.key)).collect();
        format!("non reconnu après installation ; applications vues : {noms:?}")
    })?;
    let etat = status_of(Some(entry), tool.version.as_deref());
    let version = match &etat {
        InstallStatus::Installed { version } => version.clone().unwrap_or_else(|| "?".into()),
        autre => return Err(format!("état inattendu : {autre:?} (catalogue {:?})", tool.version)),
    };

    // 4. Démarrage : l'outil doit rester ouvert.
    let exe = main_exe(entry, None).ok_or("exécutable introuvable")?;
    let pid = spawn_app(&exe).map_err(|e| format!("démarrage : {e}"))?;
    std::thread::sleep(TENUE);
    let ouvert = vivant(pid);
    arreter(pid);
    if !ouvert {
        return Err(format!("démarrage : fermé avant {} s ({})", TENUE.as_secs(), exe.display()));
    }

    // 5. Désinstallation.
    remove_app(entry, &apps, |_| false).map_err(|e| format!("désinstallation : {e}"))?;
    if app.exists() || entry_for(&MacApps { dir: apps }.entries(), tool).is_some() {
        return Err("désinstallation : application toujours présente".into());
    }
    Ok(format!("version {version}, reconnu ({}), démarré, désinstallé", entry.key))
}

#[test]
#[ignore = "réseau et vrai Mac : lancé par le workflow dédié"]
fn installe_chaque_outil_mac_du_catalogue() {
    let puce = if cfg!(target_arch = "aarch64") { "puce Apple" } else { "Intel" };
    let filtre: Option<Vec<String>> =
        std::env::var("BW_OUTILS").ok().filter(|s| !s.trim().is_empty()).map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let outils: Vec<Tool> = catalogue()
        .into_iter()
        .filter(|t| t.download.is_some())
        .filter(|t| filtre.as_ref().is_none_or(|f| f.contains(&t.id)))
        .collect();
    assert!(!outils.is_empty(), "aucun outil avec une version Mac dans le catalogue");

    println!("\n=== Mac {puce} : {} outil(s) à tester ===", outils.len());
    let mut echecs = Vec::new();
    let mut lignes = Vec::new();
    for tool in &outils {
        let work = tempfile::tempdir().expect("dossier temporaire");
        let res = essayer(tool, work.path());
        let ligne = match &res {
            Ok(r) => format!("| ✅ | {} | {} |", tool.name, r),
            Err(e) => format!("| ❌ | {} | {} |", tool.name, e.replace('|', "/")),
        };
        println!("{ligne}");
        lignes.push(ligne);
        if res.is_err() {
            echecs.push(tool.name.clone());
        }
    }

    // Résumé pour la page du workflow.
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        let mut s = format!("### Mac {puce}\n\n| | Outil | Résultat |\n|---|---|---|\n");
        for l in &lignes {
            s.push_str(l);
            s.push('\n');
        }
        let _ = std::fs::OpenOptions::new().append(true).create(true).open(path).and_then(|mut f| {
            use std::io::Write;
            f.write_all(s.as_bytes())
        });
    }
    assert!(echecs.is_empty(), "échec pour : {}", echecs.join(", "));
}
