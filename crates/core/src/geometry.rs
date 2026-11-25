//! Géométrie des fenêtres : placement par écran, replacement, maintien dans l'écran.
//!
//! - Les rectangles réels sont en pixels physiques (ce que donne Windows).
//! - Une position sauvegardée est en pixels logiques, relative à la zone de travail
//!   de son écran : elle reste juste si la mise à l'échelle change (125 %, 150 %…).
//! - Écran disparu : replacement sur l'écran principal, jamais hors de l'écran.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Rectangle en pixels physiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    fn right(&self) -> i64 {
        self.x as i64 + self.w as i64
    }

    fn bottom(&self) -> i64 {
        self.y as i64 + self.h as i64
    }

    pub fn intersection_area(&self, o: &Rect) -> i64 {
        let w = self.right().min(o.right()) - (self.x.max(o.x) as i64);
        let h = self.bottom().min(o.bottom()) - (self.y.max(o.y) as i64);
        if w > 0 && h > 0 { w * h } else { 0 }
    }
}

/// Un écran tel que vu par le launcher.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    /// Identifiant stable pendant que la configuration ne change pas.
    pub key: String,
    /// Zone de travail (sans la barre des tâches), en pixels physiques.
    pub work: Rect,
    pub scale: f64,
    pub primary: bool,
}

/// Position et taille sauvegardées, en pixels logiques relatifs à la zone de travail.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Placements d'un élément, un par écran, plus le dernier écran utilisé.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PerMonitor {
    pub placements: BTreeMap<String, Placement>,
    pub last_monitor: Option<String>,
}

impl PerMonitor {
    /// Mémorise un rectangle réel sur l'écran où il se trouve.
    pub fn remember(&mut self, rect: Rect, monitors: &[MonitorInfo]) {
        if let Some(m) = monitor_for(rect, monitors) {
            self.placements.insert(m.key.clone(), to_placement(rect, m));
            self.last_monitor = Some(m.key.clone());
        }
    }

    /// Rectangle où rouvrir l'élément (voir [`restore`]).
    pub fn restore(&self, monitors: &[MonitorInfo], default: Placement) -> Option<Rect> {
        restore(self, monitors, default)
    }
}

pub fn to_placement(rect: Rect, m: &MonitorInfo) -> Placement {
    let s = if m.scale > 0.0 { m.scale } else { 1.0 };
    Placement {
        x: (rect.x - m.work.x) as f64 / s,
        y: (rect.y - m.work.y) as f64 / s,
        w: rect.w as f64 / s,
        h: rect.h as f64 / s,
    }
}

pub fn from_placement(p: &Placement, m: &MonitorInfo) -> Rect {
    let s = if m.scale > 0.0 { m.scale } else { 1.0 };
    Rect {
        x: m.work.x + (p.x * s).round() as i32,
        y: m.work.y + (p.y * s).round() as i32,
        w: (p.w * s).round().max(1.0) as u32,
        h: (p.h * s).round().max(1.0) as u32,
    }
}

/// Écran qui contient la plus grande partie du rectangle (sinon : le plus proche de son centre).
pub fn monitor_for(rect: Rect, monitors: &[MonitorInfo]) -> Option<&MonitorInfo> {
    let best = monitors.iter().max_by_key(|m| rect.intersection_area(&m.work));
    match best {
        Some(m) if rect.intersection_area(&m.work) > 0 => Some(m),
        _ => {
            let cx = rect.x as i64 + rect.w as i64 / 2;
            let cy = rect.y as i64 + rect.h as i64 / 2;
            monitors.iter().min_by_key(|m| {
                let mx = m.work.x as i64 + m.work.w as i64 / 2;
                let my = m.work.y as i64 + m.work.h as i64 / 2;
                (mx - cx).pow(2) + (my - cy).pow(2)
            })
        }
    }
}

/// Ramène le rectangle entièrement dans la zone de travail (en le réduisant si besoin).
pub fn clamp(rect: Rect, m: &MonitorInfo) -> Rect {
    let w = rect.w.min(m.work.w);
    let h = rect.h.min(m.work.h);
    let max_x = m.work.x as i64 + m.work.w as i64 - w as i64;
    let max_y = m.work.y as i64 + m.work.h as i64 - h as i64;
    Rect {
        x: (rect.x as i64).clamp(m.work.x as i64, max_x) as i32,
        y: (rect.y as i64).clamp(m.work.y as i64, max_y) as i32,
        w,
        h,
    }
}

pub fn primary(monitors: &[MonitorInfo]) -> Option<&MonitorInfo> {
    monitors.iter().find(|m| m.primary).or_else(|| monitors.first())
}

/// Où rouvrir un élément :
/// 1. sur son dernier écran s'il est encore branché ;
/// 2. sinon sur l'écran principal (à sa place mémorisée pour cet écran, ou par défaut).
///
/// Le résultat est toujours ramené dans la zone de travail.
pub fn restore(saved: &PerMonitor, monitors: &[MonitorInfo], default: Placement) -> Option<Rect> {
    if let Some(key) = &saved.last_monitor {
        if let (Some(m), Some(p)) = (monitors.iter().find(|m| &m.key == key), saved.placements.get(key)) {
            return Some(clamp(from_placement(p, m), m));
        }
    }
    let m = primary(monitors)?;
    let p = saved.placements.get(&m.key).copied().unwrap_or(default);
    Some(clamp(from_placement(&p, m), m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ecran(key: &str, x: i32, w: u32, h: u32, scale: f64, primary: bool) -> MonitorInfo {
        MonitorInfo { key: key.into(), work: Rect::new(x, 0, w, h), scale, primary }
    }

    fn deux_ecrans() -> Vec<MonitorInfo> {
        vec![ecran("A", 0, 1920, 1040, 1.0, true), ecran("B", 1920, 2560, 1400, 1.5, false)]
    }

    const DEFAUT: Placement = Placement { x: 20.0, y: 20.0, w: 300.0, h: 400.0 };

    #[test]
    fn aller_retour_avec_mise_a_l_echelle() {
        let ecrans = deux_ecrans();
        let r = Rect::new(1920 + 300, 150, 600, 450);
        let p = to_placement(r, &ecrans[1]);
        assert_eq!(p, Placement { x: 200.0, y: 100.0, w: 400.0, h: 300.0 });
        assert_eq!(from_placement(&p, &ecrans[1]), r);
    }

    #[test]
    fn position_memorisee_par_ecran() {
        let ecrans = deux_ecrans();
        let mut s = PerMonitor::default();
        s.remember(Rect::new(100, 100, 300, 400), &ecrans);
        s.remember(Rect::new(2000, 100, 450, 600), &ecrans);
        assert_eq!(s.placements.len(), 2);
        assert_eq!(s.last_monitor.as_deref(), Some("B"));
        assert_eq!(s.restore(&ecrans, DEFAUT), Some(Rect::new(2000, 100, 450, 600)));
    }

    #[test]
    fn ecran_debranche_replace_sur_le_principal() {
        let ecrans = deux_ecrans();
        let mut s = PerMonitor::default();
        s.remember(Rect::new(100, 100, 300, 400), &ecrans);
        s.remember(Rect::new(4000, 900, 450, 600), &ecrans);
        // L'écran B a disparu : retour à la place mémorisée sur A.
        let seul_a = vec![ecrans[0].clone()];
        assert_eq!(s.restore(&seul_a, DEFAUT), Some(Rect::new(100, 100, 300, 400)));
    }

    #[test]
    fn jamais_hors_de_l_ecran() {
        let a = ecran("A", 0, 1920, 1040, 1.0, true);
        assert_eq!(clamp(Rect::new(1800, 1000, 300, 400), &a), Rect::new(1620, 640, 300, 400));
        assert_eq!(clamp(Rect::new(-500, -50, 300, 400), &a), Rect::new(0, 0, 300, 400));
        // Plus grand que l'écran : réduit.
        assert_eq!(clamp(Rect::new(0, 0, 3000, 2000), &a), Rect::new(0, 0, 1920, 1040));
        // Rien de mémorisé pour l'écran principal : position par défaut.
        let s = PerMonitor { last_monitor: Some("disparu".into()), ..Default::default() };
        assert_eq!(s.restore(&[a], DEFAUT), Some(Rect::new(20, 20, 300, 400)));
    }

    #[test]
    fn ecran_d_un_rectangle() {
        let ecrans = deux_ecrans();
        assert_eq!(monitor_for(Rect::new(1800, 0, 400, 100), &ecrans).unwrap().key, "B");
        // Hors de tout écran : le plus proche.
        assert_eq!(monitor_for(Rect::new(-3000, 0, 10, 10), &ecrans).unwrap().key, "A");
        assert!(monitor_for(Rect::new(0, 0, 1, 1), &[]).is_none());
    }
}
