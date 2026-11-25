//! Lancement au démarrage de Windows : une seule valeur dans
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, nommée « Bworkshop Launcher ».
//!
//! Le chemin de l'exécutable est **entre guillemets** : il contient une espace
//! (`…\Bworkshop Launcher\…`). Sans guillemets, Windows essaierait d'abord
//! `…\Local\Bworkshop` (lancement peu fiable, et faille connue des « chemins sans guillemets »).
//! La désinstallation supprime cette valeur (`windows/hooks.nsh`).

use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, REG_VALUE_TYPE,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Bworkshop Launcher";
/// Ancien nom possible (versions de test) : retiré s'il existe.
const LEGACY_NAME: &str = "bworkshop-launcher";

/// Ligne de commande enregistrée : chemin entre guillemets, puis l'argument.
pub fn command_line(exe: &Path, arg: &str) -> String {
    format!("\"{}\" {arg}", exe.display())
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn open() -> Result<Key, String> {
    let mut key = HKEY::default();
    let path = HSTRING::from(RUN_KEY);
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(path.as_ptr()), None, KEY_QUERY_VALUE | KEY_SET_VALUE, &mut key) }
        .ok()
        .map_err(|e| e.message())?;
    Ok(Key(key))
}

fn read(key: &Key, name: &str) -> Option<String> {
    let name = HSTRING::from(name);
    let mut kind = REG_VALUE_TYPE::default();
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    unsafe {
        RegQueryValueExW(key.0, PCWSTR(name.as_ptr()), None, Some(&mut kind), Some(buf.as_mut_ptr().cast()), Some(&mut size))
    }
    .ok()
    .ok()?;
    let len = (size as usize / 2).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]).trim_end_matches('\0').to_string())
}

fn delete(key: &Key, name: &str) -> Result<(), String> {
    let name = HSTRING::from(name);
    let r = unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) };
    if r.is_ok() || r == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(r.to_hresult().message())
    }
}

/// Aligne la valeur sur le réglage. Renvoie vrai si quelque chose a changé.
pub fn sync(enabled: bool, exe: &Path, arg: &str) -> Result<bool, String> {
    let key = open()?;
    let mut changed = false;
    if read(&key, LEGACY_NAME).is_some() {
        delete(&key, LEGACY_NAME)?;
        changed = true;
    }
    let current = read(&key, VALUE_NAME);
    if enabled {
        let wanted = command_line(exe, arg);
        if current.as_deref() != Some(wanted.as_str()) {
            let name = HSTRING::from(VALUE_NAME);
            let mut data: Vec<u16> = wanted.encode_utf16().collect();
            data.push(0);
            let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
            unsafe { RegSetValueExW(key.0, PCWSTR(name.as_ptr()), None, REG_SZ, Some(bytes)) }
                .ok()
                .map_err(|e| e.message())?;
            changed = true;
        }
    } else if current.is_some() {
        delete(&key, VALUE_NAME)?;
        changed = true;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemin_entre_guillemets() {
        let exe = Path::new(r"C:\Users\Moi\AppData\Local\Bworkshop Launcher\bworkshop-launcher.exe");
        assert_eq!(
            command_line(exe, "--hidden"),
            r#""C:\Users\Moi\AppData\Local\Bworkshop Launcher\bworkshop-launcher.exe" --hidden"#
        );
    }
}
