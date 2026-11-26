//! Raccourcis clavier globaux : lecture du texte (« Alt+B »), règles, conflits.
//!
//! Les raccourcis sont seulement **écoutés** par le launcher (`RegisterHotKey`),
//! jamais simulés. Ce module ne dépend pas de Windows : il produit les modificateurs
//! et le code de touche virtuelle à donner à `RegisterHotKey`.

use crate::settings::HotkeyBindings;
use std::fmt;

/// Modificateurs, mêmes valeurs que `MOD_ALT`, `MOD_CONTROL`, `MOD_SHIFT`, `MOD_WIN`.
pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;

/// Un raccourci : modificateurs + touche virtuelle Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub modifiers: u32,
    pub vk: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyError {
    Empty,
    UnknownKey(String),
    /// Seulement des modificateurs, ou deux touches.
    NeedOneKey,
    /// Une touche seule (hors F1–F24) gênerait la frappe partout.
    NeedModifier,
    /// Raccourci réservé par Windows ou trop risqué (Alt+F4, Alt+Tab…).
    Reserved,
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Empty => write!(f, "raccourci vide"),
            HotkeyError::UnknownKey(k) => write!(f, "touche inconnue : {k}"),
            HotkeyError::NeedOneKey => write!(f, "il faut une seule touche en plus de Ctrl, Alt, Maj ou Win"),
            HotkeyError::NeedModifier => write!(f, "il faut Ctrl, Alt ou Win avec la touche (sauf F1 à F24)"),
            HotkeyError::Reserved => write!(f, "raccourci réservé par Windows"),
        }
    }
}

impl std::error::Error for HotkeyError {}

/// Noms de touches acceptés → code de touche virtuelle.
fn key_code(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&n) {
            return Some(0x70 + n - 1);
        }
    if let Some(n) = upper.strip_prefix("NUM").and_then(|n| n.parse::<u32>().ok())
        && n <= 9 {
            return Some(0x60 + n);
        }
    Some(match upper.as_str() {
        "SPACE" => 0x20,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "END" => 0x23,
        "HOME" => 0x24,
        "LEFT" => 0x25,
        "UP" => 0x26,
        "RIGHT" => 0x27,
        "DOWN" => 0x28,
        "INSERT" => 0x2D,
        "DELETE" => 0x2E,
        _ => return None,
    })
}

/// Nom affiché d'une touche virtuelle (inverse de `key_code`).
fn key_name(vk: u32) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(String::from).unwrap_or_default(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x20 => "Space".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        _ => format!("#{vk}"),
    }
}

fn is_function_key(vk: u32) -> bool {
    (0x70..=0x87).contains(&vk)
}

impl Hotkey {
    /// Lit « Ctrl+Alt+B », « alt + 1 », « Win+F5 »… (casse et espaces libres).
    pub fn parse(text: &str) -> Result<Self, HotkeyError> {
        let parts: Vec<&str> = text.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err(HotkeyError::Empty);
        }
        let mut modifiers = 0;
        let mut key = None;
        for part in parts {
            let m = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => MOD_CONTROL,
                "alt" => MOD_ALT,
                "shift" | "maj" => MOD_SHIFT,
                "win" | "super" | "meta" => MOD_WIN,
                _ => 0,
            };
            if m != 0 {
                modifiers |= m;
                continue;
            }
            if key.is_some() {
                return Err(HotkeyError::NeedOneKey);
            }
            key = Some(key_code(part).ok_or_else(|| HotkeyError::UnknownKey(part.to_string()))?);
        }
        let vk = key.ok_or(HotkeyError::NeedOneKey)?;
        let hotkey = Hotkey { modifiers, vk };
        hotkey.check()?;
        Ok(hotkey)
    }

    fn check(&self) -> Result<(), HotkeyError> {
        let strong = self.modifiers & (MOD_CONTROL | MOD_ALT | MOD_WIN) != 0;
        if !strong && !is_function_key(self.vk) {
            return Err(HotkeyError::NeedModifier);
        }
        let reserved = [
            Hotkey { modifiers: MOD_ALT, vk: 0x73 },             // Alt+F4
            Hotkey { modifiers: MOD_ALT, vk: 0x09 },             // Alt+Tab
            Hotkey { modifiers: MOD_ALT, vk: 0x20 },             // Alt+Espace (menu système)
            Hotkey { modifiers: MOD_CONTROL | MOD_ALT, vk: 0x2E }, // Ctrl+Alt+Suppr
            Hotkey { modifiers: MOD_WIN, vk: 0x4C },             // Win+L
            Hotkey { modifiers: MOD_WIN, vk: 0x44 },             // Win+D
        ];
        if self.modifiers & MOD_WIN != 0 && self.modifiers & !MOD_WIN == 0 {
            // Win + lettre : presque tous pris par Windows.
            if (0x41..=0x5A).contains(&self.vk) {
                return Err(HotkeyError::Reserved);
            }
        }
        if reserved.contains(self) {
            return Err(HotkeyError::Reserved);
        }
        Ok(())
    }
}

/// Forme normalisée : « Ctrl+Alt+Shift+Win+Touche ».
impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (m, name) in [(MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift"), (MOD_WIN, "Win")] {
            if self.modifiers & m != 0 {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", key_name(self.vk))
    }
}

/// Actions qui ont un raccourci aujourd'hui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Afficher / cacher la barre d'outils.
    TogglePanel,
    /// Tout cacher / tout montrer.
    ToggleHideAll,
    /// Verrouiller / déverrouiller les versions web.
    ToggleLock,
}

impl Action {
    pub const ALL: [Action; 3] = [Action::TogglePanel, Action::ToggleHideAll, Action::ToggleLock];

    /// Nom utilisé dans les réglages et l'interface.
    pub fn key(self) -> &'static str {
        match self {
            Action::TogglePanel => "togglePanel",
            Action::ToggleHideAll => "toggleHideAll",
            Action::ToggleLock => "toggleLock",
        }
    }

    pub fn binding(self, b: &HotkeyBindings) -> Option<&str> {
        match self {
            Action::TogglePanel => b.toggle_panel.as_deref(),
            Action::ToggleHideAll => b.toggle_hide_all.as_deref(),
            Action::ToggleLock => b.toggle_lock.as_deref(),
        }
    }
}

/// Ce que déclenche un raccourci : une action du launcher, ou un outil (ouvrir / fermer).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Action(Action),
    /// Disposition n (1 à 4).
    Layout(usize),
    Tool(String),
}

impl Target {
    /// Nom utilisé par l'interface : « togglePanel »… ou « tool:<id> ».
    pub fn key(&self) -> String {
        match self {
            Target::Action(a) => a.key().to_string(),
            Target::Layout(n) => format!("layout:{n}"),
            Target::Tool(id) => format!("tool:{id}"),
        }
    }
}

/// Raccourcis à enregistrer, et problèmes trouvés avant même de demander à Windows.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub register: Vec<(Target, Hotkey)>,
    pub problems: Vec<(Target, String)>,
}

/// Nombre maximal de raccourcis d'outils (garde-fou contre un fichier de réglages trafiqué).
pub const MAX_TOOL_HOTKEYS: usize = 32;

/// Lit les raccourcis des réglages : invalides et doublons sont signalés, pas enregistrés.
/// Les actions du launcher passent avant les outils.
pub fn plan(bindings: &HotkeyBindings) -> Plan {
    let mut plan = Plan::default();
    let actions = Action::ALL.iter().filter_map(|a| a.binding(bindings).map(|t| (Target::Action(*a), t)));
    let layouts = bindings
        .layouts
        .iter()
        .take(crate::settings::LAYOUT_HOTKEY_COUNT)
        .enumerate()
        .filter_map(|(i, t)| t.as_deref().map(|t| (Target::Layout(i + 1), t)));
    let tools = bindings.widgets.iter().take(MAX_TOOL_HOTKEYS).map(|(id, t)| (Target::Tool(id.clone()), t.as_str()));
    for (target, text) in actions.chain(layouts).chain(tools) {
        if text.trim().is_empty() {
            continue;
        }
        match Hotkey::parse(text) {
            Err(e) => plan.problems.push((target, e.to_string())),
            Ok(h) => {
                if let Some((other, _)) = plan.register.iter().find(|(_, x)| *x == h) {
                    let msg = format!("déjà utilisé pour « {} »", other.key());
                    plan.problems.push((target, msg));
                } else {
                    plan.register.push((target, h));
                }
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lecture_et_forme_normalisee() {
        let h = Hotkey::parse("alt + b").unwrap();
        assert_eq!(h, Hotkey { modifiers: MOD_ALT, vk: 0x42 });
        assert_eq!(h.to_string(), "Alt+B");
        assert_eq!(Hotkey::parse("Shift+Ctrl+1").unwrap().to_string(), "Ctrl+Shift+1");
        assert_eq!(Hotkey::parse("F5").unwrap().to_string(), "F5");
        assert_eq!(Hotkey::parse("Alt+Num3").unwrap().vk, 0x63);
        assert_eq!(Hotkey::parse("Ctrl+PageUp").unwrap().to_string(), "Ctrl+PageUp");
    }

    #[test]
    fn refus() {
        assert_eq!(Hotkey::parse(""), Err(HotkeyError::Empty));
        assert_eq!(Hotkey::parse("Alt"), Err(HotkeyError::NeedOneKey));
        assert_eq!(Hotkey::parse("Alt+A+B"), Err(HotkeyError::NeedOneKey));
        assert_eq!(Hotkey::parse("B"), Err(HotkeyError::NeedModifier));
        assert_eq!(Hotkey::parse("Shift+B"), Err(HotkeyError::NeedModifier));
        assert_eq!(Hotkey::parse("Alt+F4"), Err(HotkeyError::Reserved));
        assert_eq!(Hotkey::parse("Win+L"), Err(HotkeyError::Reserved));
        assert!(matches!(Hotkey::parse("Alt+é"), Err(HotkeyError::UnknownKey(_))));
    }

    #[test]
    fn plan_par_defaut_et_doublons() {
        // Alt+B, Alt+H, Alt+L, puis les dispositions Alt+1 à Alt+4.
        let p = plan(&HotkeyBindings::default());
        assert_eq!(p.register.len(), 7);
        assert!(p.problems.is_empty());
        assert_eq!(p.register[3], (Target::Layout(1), Hotkey { modifiers: MOD_ALT, vk: 0x31 }));
        assert_eq!(Target::Layout(4).key(), "layout:4");

        let b = HotkeyBindings {
            toggle_panel: Some("Alt+B".into()),
            toggle_hide_all: Some("alt+b".into()),
            toggle_lock: None,
            ..HotkeyBindings::default()
        };
        let p = plan(&b);
        assert_eq!(p.register.len(), 5);
        assert_eq!(p.problems.len(), 1);
        assert_eq!(p.problems[0].0, Target::Action(Action::ToggleHideAll));
    }

    #[test]
    fn raccourcis_d_outils() {
        let mut b = HotkeyBindings::default();
        b.widgets.insert("grimoire".into(), "Alt+G".into());
        b.widgets.insert("harebourg".into(), "Alt+B".into()); // pris par la barre
        let p = plan(&b);
        assert!(p.register.contains(&(Target::Tool("grimoire".into()), Hotkey { modifiers: MOD_ALT, vk: 0x47 })));
        assert_eq!(p.problems, vec![(Target::Tool("harebourg".into()), "déjà utilisé pour « togglePanel »".into())]);
        assert_eq!(Target::Tool("grimoire".into()).key(), "tool:grimoire");
    }
}
