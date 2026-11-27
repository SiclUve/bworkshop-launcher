//! Lancement à l'ouverture de session sur Mac : un seul fichier,
//! `~/Library/LaunchAgents/app.bworkshop.launcher.plist`, lu par macOS (`launchd`) à la
//! connexion. Il lance l'exécutable du launcher avec `--hidden` (reste près de l'horloge).
//!
//! Le chemin est réécrit à chaque démarrage s'il a changé (application déplacée).

use std::path::{Path, PathBuf};

const LABEL: &str = "app.bworkshop.launcher";

fn agent_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("dossier personnel introuvable")?;
    Ok(Path::new(&home).join("Library").join("LaunchAgents").join(format!("{LABEL}.plist")))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Contenu du fichier : programme et argument, lancé une fois à la connexion.
pub fn agent_plist(exe: &Path, arg: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>{}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#,
        xml_escape(&exe.to_string_lossy()),
        xml_escape(arg)
    )
}

/// Aligne le fichier sur le réglage. Renvoie vrai si quelque chose a changé.
pub fn sync(enabled: bool, exe: &Path, arg: &str) -> Result<bool, String> {
    let path = agent_path()?;
    let current = std::fs::read_to_string(&path).ok();
    if enabled {
        let wanted = agent_plist(exe, arg);
        if current.as_deref() == Some(wanted.as_str()) {
            return Ok(false);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, wanted).map_err(|e| e.to_string())?;
        Ok(true)
    } else if current.is_some() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichier_attendu() {
        let p = agent_plist(Path::new("/Applications/Bworkshop Launcher.app/Contents/MacOS/bworkshop-launcher"), "--hidden");
        assert!(p.contains("<string>/Applications/Bworkshop Launcher.app/Contents/MacOS/bworkshop-launcher</string>"));
        assert!(p.contains("<string>--hidden</string>"));
        assert!(p.contains("<key>RunAtLoad</key>\n\t<true/>"));
    }
}
