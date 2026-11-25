//! Tests d'intégration réels : installent, mettent à jour, lancent puis désinstallent
//! les petits installateurs de `test-assets` (Inno Setup et NSIS), pour l'utilisateur
//! seul, sans droits admin.
//!
//! Ils modifient le registre de l'utilisateur (HKCU) et installent dans
//! `%LOCALAPPDATA%\BworkshopLauncherTest` : ils sont donc ignorés par défaut.
//! Lancer avec : `cargo test -p bworkshop-core --test installer_integration -- --ignored --test-threads=1`

use bworkshop_core::installer::kind::{self, InstallerKind};
use bworkshop_core::installer::launch;
use bworkshop_core::installer::registry::{self, RegistrySource, UninstallEntry, WindowsRegistry};
use bworkshop_core::installer::runner;
use bworkshop_core::installer::status::{status_of, InstallStatus};
use bworkshop_core::security::hash;
use std::path::PathBuf;
use std::time::Duration;

struct Case {
    kind: InstallerKind,
    /// Fichier relatif à `test-assets`, `{v}` remplacé par la version.
    asset: &'static str,
    app_id: Option<&'static str>,
    name: &'static str,
}

const INNO: Case = Case {
    kind: InstallerKind::Inno,
    asset: "inno/TestTool_Setup_{v}.exe",
    app_id: Some("{0B5E7A31-6C2D-4E8F-9A10-B4C3D2E1F007}"),
    name: "Bworkshop Test Tool",
};

const NSIS: Case = Case {
    kind: InstallerKind::Nsis,
    asset: "nsis/NsisTool_Setup_{v}.exe",
    app_id: None,
    name: "Bworkshop NSIS Test Tool",
};

impl Case {
    fn asset(&self, v: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-assets").join(self.asset.replace("{v}", v))
    }

    fn installed(&self) -> Option<UninstallEntry> {
        registry::find(&WindowsRegistry.entries(), self.app_id, self.name).cloned()
    }

    fn uninstall(&self, e: &UninstallEntry) {
        runner::uninstall(e, Duration::from_secs(60), || self.installed().is_none()).unwrap();
    }

    fn run(&self) {
        // Point de départ propre.
        if let Some(e) = self.installed() {
            self.uninstall(&e);
        }
        assert_eq!(status_of(self.installed().as_ref(), Some("1.1.0")), InstallStatus::NotInstalled);

        // Le type est reconnu par la signature ; l'empreinte est calculable.
        let v1 = self.asset("1.0.0");
        assert_eq!(kind::detect(&v1).unwrap(), Some(self.kind));
        assert_eq!(hash::sha256_file(&v1).unwrap().len(), 64);

        // Installation 1.0.0.
        runner::install(self.kind, &v1).unwrap();
        let e = self.installed().expect("1.0.0 doit apparaître dans le registre");
        assert_eq!(e.display_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            status_of(Some(&e), Some("1.1.0")),
            InstallStatus::UpdateAvailable { installed: "1.0.0".into(), available: "1.1.0".into() }
        );

        // Mise à jour 1.1.0.
        runner::install(self.kind, &self.asset("1.1.0")).unwrap();
        let e = self.installed().expect("1.1.0 doit apparaître dans le registre");
        assert_eq!(e.display_version.as_deref(), Some("1.1.0"));
        assert_eq!(status_of(Some(&e), Some("1.1.0")), InstallStatus::Installed { version: Some("1.1.0".into()) });

        // Lancement de l'exécutable principal (lu depuis DisplayIcon).
        let exe = launch::main_exe(&e, None).expect("exécutable principal");
        assert!(exe.ends_with("testtool.exe"), "{}", exe.display());
        // Le programme de test se ferme tout seul.
        let status = launch::launch(&exe).unwrap().wait().unwrap();
        assert!(status.success());

        // Désinstallation.
        self.uninstall(&e);
        assert!(self.installed().is_none());
        std::thread::sleep(Duration::from_secs(2)); // le désinstalleur finit de nettoyer
        assert!(!exe.exists(), "les fichiers doivent être supprimés");
    }
}

#[test]
#[ignore = "modifie le registre de l'utilisateur"]
fn inno_installer_mettre_a_jour_lancer_desinstaller() {
    INNO.run();
}

#[test]
#[ignore = "modifie le registre de l'utilisateur"]
fn nsis_installer_mettre_a_jour_lancer_desinstaller() {
    NSIS.run();
}
