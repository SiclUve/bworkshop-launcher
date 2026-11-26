//! Reconnaissance de Dofus par le **titre** de sa fenêtre, rien d'autre, et calculs
//! du suivi (option « Suivre Dofus ») : aimantation et déplacement des widgets.
//! Les appels à Windows sont dans l'application (`src-tauri/src/dofus.rs`).

use crate::geometry::Rect;

/// Motif par défaut si le réglage est vide (le même que celui des réglages).
use crate::settings::DEFAULT_DOFUS_PATTERN as DEFAULT_PATTERN;

/// Distance d'aimantation, en pixels logiques (multipliée par l'échelle de l'écran).
pub const SNAP_DISTANCE: f64 = 12.0;

/// Le titre correspond-il au motif (sans tenir compte des majuscules) ?
///
/// Avec le motif par défaut, le titre de Dofus 3 en jeu est aussi reconnu : il ne contient
/// pas « Dofus » mais « Personnage - Classe - 3.7.14.18 - Release ». Un motif personnalisé
/// (par exemple le nom d'un personnage, pour ne suivre qu'un compte) est pris tel quel.
pub fn title_matches(title: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    let is_default = pattern.is_empty() || pattern.eq_ignore_ascii_case(DEFAULT_PATTERN);
    let pattern = if pattern.is_empty() { DEFAULT_PATTERN } else { pattern };
    title.to_lowercase().contains(&pattern.to_lowercase()) || (is_default && is_dofus3_title(title))
}

/// Titre de Dofus 3 en jeu : « … - <version a.b.c…> - Release » (ou « Beta »).
fn is_dofus3_title(title: &str) -> bool {
    let parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    if parts.len() < 3 {
        return false;
    }
    let channel = parts[parts.len() - 1];
    let version = parts[parts.len() - 2];
    let is_version = version.split('.').count() >= 3 && version.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    is_version && (channel.eq_ignore_ascii_case("Release") || channel.eq_ignore_ascii_case("Beta"))
}

/// Fenêtre réduite : Windows la place vers (-32000, -32000).
pub fn is_minimized_rect(r: Rect) -> bool {
    r.x <= -30000 || r.y <= -30000
}

/// Plus petit décalage (en valeur absolue) qui colle un des bords `a` à un des bords `b`,
/// s'il est au plus de `distance`.
fn closest(a: [i64; 2], b: [i64; 2], distance: i64) -> Option<i64> {
    a.iter()
        .flat_map(|x| b.iter().map(move |y| y - x))
        .filter(|d| d.abs() <= distance)
        .min_by_key(|d| d.abs())
}

/// Décalage (dx, dy) qui aimante `moving` aux bords de `target` (dedans comme dehors), s'il
/// en est à `distance` pixels au plus. Un axe n'est aimanté que si les deux rectangles se
/// font face sur l'autre axe (sinon on collerait à un bord lointain).
fn snap_offset(moving: Rect, target: Rect, d: i64) -> (Option<i64>, Option<i64>) {
    let (mx, my, mw, mh) = (moving.x as i64, moving.y as i64, moving.w as i64, moving.h as i64);
    let (tx, ty, tw, th) = (target.x as i64, target.y as i64, target.w as i64, target.h as i64);
    let face_y = my <= ty + th + d && my + mh >= ty - d;
    let face_x = mx <= tx + tw + d && mx + mw >= tx - d;
    let dx = if face_y { closest([mx, mx + mw], [tx, tx + tw], d) } else { None };
    let dy = if face_x { closest([my, my + mh], [ty, ty + th], d) } else { None };
    (dx, dy)
}

/// Aimante `moving` au plus proche des bords de `targets` (jeu, écran, autres fenêtres du
/// launcher), axe par axe. La taille ne change jamais.
pub fn snap_many(moving: Rect, targets: &[Rect], distance: i32) -> Rect {
    let d = distance.max(0) as i64;
    let best = |a: Option<i64>, b: Option<i64>| match (a, b) {
        (Some(x), Some(y)) => Some(if y.abs() < x.abs() { y } else { x }),
        (x, None) => x,
        (None, y) => y,
    };
    let (mut dx, mut dy) = (None, None);
    for t in targets {
        let (x, y) = snap_offset(moving, *t, d);
        dx = best(dx, x);
        dy = best(dy, y);
    }
    Rect::new(
        (moving.x as i64 + dx.unwrap_or(0)) as i32,
        (moving.y as i64 + dy.unwrap_or(0)) as i32,
        moving.w,
        moving.h,
    )
}

/// Aimante `moving` aux bords d'un seul rectangle (tests).
#[cfg(test)]
fn snap(moving: Rect, target: Rect, distance: i32) -> Rect {
    snap_many(moving, &[target], distance)
}

/// Nouvelle place d'un widget quand Dofus passe de `before` à `after` :
/// seulement si le widget touchait ou chevauchait Dofus, et si Dofus a seulement
/// été déplacé (même taille). Réduire, agrandir ou redimensionner le jeu ne bouge rien.
pub fn follow(widget: Rect, before: Rect, after: Rect, distance: i32) -> Option<Rect> {
    if before == after || before.w != after.w || before.h != after.h {
        return None;
    }
    if is_minimized_rect(before) || is_minimized_rect(after) {
        return None;
    }
    let d = distance.max(0);
    let grown = Rect::new(
        before.x - d,
        before.y - d,
        before.w + 2 * d as u32,
        before.h + 2 * d as u32,
    );
    if widget.intersection_area(&grown) == 0 {
        return None;
    }
    Some(Rect::new(widget.x + (after.x - before.x), widget.y + (after.y - before.y), widget.w, widget.h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titres() {
        assert!(title_matches("Pseudo - Dofus 3.4.12.5 - Release", "Dofus"));
        assert!(title_matches("DOFUS", "dofus"));
        assert!(title_matches("Dofus Retro", ""));
        assert!(!title_matches("Bworklingo", "Dofus"));
        assert!(!title_matches("", "Dofus"));
        // Dofus 3 en jeu : le titre ne contient pas « Dofus ».
        assert!(title_matches("Baik - Ecaflip - 3.7.14.18 - Release", "Dofus"));
        assert!(title_matches("Baik - Ecaflip - 3.7.14.18 - Release", ""));
        assert!(title_matches("Perso - Iop - 3.8.0.1 - Beta", "Dofus"));
        // Motif personnalisé : seulement ce motif (suivre un seul personnage).
        assert!(title_matches("Baik - Ecaflip - 3.7.14.18 - Release", "baik"));
        assert!(!title_matches("Autre - Iop - 3.7.14.18 - Release", "Baik"));
        // Ressemblances refusées.
        assert!(!title_matches("Notes - v3 - Release", "Dofus"));
        assert!(!title_matches("Projet - 1.2 - Release", "Dofus"));
        assert!(!title_matches("Release - 3.7.14.18", "Dofus"));
    }

    const JEU: Rect = Rect { x: 100, y: 100, w: 1000, h: 800 };

    #[test]
    fn aimantation_aux_bords() {
        // À 8 px à droite du bord gauche, dedans : collé au bord gauche.
        assert_eq!(snap(Rect::new(108, 300, 200, 100), JEU, 12), Rect::new(100, 300, 200, 100));
        // Dehors, à gauche : son bord droit se colle au bord gauche du jeu.
        assert_eq!(snap(Rect::new(-110, 300, 200, 100), JEU, 12), Rect::new(-100, 300, 200, 100));
        // Coin haut-droit, dedans.
        assert_eq!(snap(Rect::new(895, 95, 200, 100), JEU, 12), Rect::new(900, 100, 200, 100));
        // Trop loin : rien ne bouge.
        assert_eq!(snap(Rect::new(130, 300, 200, 100), JEU, 12), Rect::new(130, 300, 200, 100));
        // Loin en dessous du jeu : pas d'aimantation horizontale à un bord qu'on ne longe pas.
        assert_eq!(snap(Rect::new(105, 2000, 200, 100), JEU, 12), Rect::new(105, 2000, 200, 100));
    }

    #[test]
    fn aimantation_au_plus_proche() {
        let ecran = Rect::new(0, 0, 1920, 1040);
        let voisin = Rect::new(500, 300, 200, 200);
        // À 5 px à droite du voisin, et son haut à 5 px de celui du voisin : colle sur les deux axes.
        assert_eq!(snap_many(Rect::new(705, 295, 100, 100), &[ecran, voisin], 12), Rect::new(700, 300, 100, 100));
        // Près du coin haut-gauche de l'écran : collé dans le coin.
        assert_eq!(snap_many(Rect::new(4, 7, 100, 100), &[ecran, voisin], 12), Rect::new(0, 0, 100, 100));
        // Deux bords proches sur le même axe : le plus proche gagne (2 px contre 9 px).
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(111, 0, 100, 100);
        assert_eq!(snap_many(Rect::new(102, 0, 7, 50), &[a, b], 12).x, 100);
        // Aucune cible : rien ne bouge.
        assert_eq!(snap_many(Rect::new(5, 5, 10, 10), &[], 12), Rect::new(5, 5, 10, 10));
    }

    #[test]
    fn suivi_du_jeu() {
        let apres = Rect::new(150, 80, 1000, 800);
        // Widget sur le jeu : même déplacement (+50, -20).
        assert_eq!(follow(Rect::new(200, 200, 300, 300), JEU, apres, 12), Some(Rect::new(250, 180, 300, 300)));
        // Widget collé dehors, à droite : suit aussi.
        assert_eq!(follow(Rect::new(1100, 200, 300, 300), JEU, apres, 12), Some(Rect::new(1150, 180, 300, 300)));
        // Widget loin du jeu : ne bouge pas.
        assert_eq!(follow(Rect::new(1500, 200, 300, 300), JEU, apres, 12), None);
        // Jeu redimensionné, réduit ou immobile : rien.
        assert_eq!(follow(Rect::new(200, 200, 300, 300), JEU, Rect::new(100, 100, 1200, 800), 12), None);
        assert_eq!(follow(Rect::new(200, 200, 300, 300), JEU, Rect::new(-32000, -32000, 1000, 800), 12), None);
        assert_eq!(follow(Rect::new(200, 200, 300, 300), JEU, JEU, 12), None);
    }
}
