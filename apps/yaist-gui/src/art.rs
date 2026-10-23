//! A line-art chameleon for the welcome screen, drawn in code so it needs no
//! image files and stays in the palette. Our own drawing, not the Geeko logo.

use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{pos2, Color32, Painter, Pos2, Rect, Stroke};

use crate::theme::Tokens;

/// Draws the chameleon on its branch, fitted into `rect` (designed at 240×170).
pub fn chameleon(p: &Painter, rect: Rect, t: &Tokens) {
    let s = (rect.width() / 240.0).min(rect.height() / 170.0);
    let at = |x: f32, y: f32| -> Pos2 { pos2(rect.left() + x * s, rect.top() + y * s) };
    let green = t.accent;
    let soft = t.accent.gamma_multiply(0.45);
    let line = Stroke::new(3.0 * s, green);
    let thin = Stroke::new(1.6 * s, soft);
    let curve = |a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32), stroke: Stroke| {
        p.add(CubicBezierShape::from_points_stroke([at(a.0, a.1), at(b.0, b.1), at(c.0, c.1), at(d.0, d.1)], false, Color32::TRANSPARENT, stroke));
    };

    // A halo behind, like the round frames of ArtCraft's app illustrations.
    p.circle_filled(at(132.0, 92.0), 78.0 * s, t.sunken);

    // The branch, with two leaves.
    curve((4.0, 152.0), (80.0, 140.0), (160.0, 158.0), (236.0, 146.0), Stroke::new(2.4 * s, t.line_strong));
    curve((206.0, 149.0), (214.0, 134.0), (228.0, 132.0), (234.0, 128.0), thin);
    curve((206.0, 149.0), (220.0, 150.0), (230.0, 140.0), (234.0, 128.0), thin);
    curve((20.0, 148.0), (14.0, 136.0), (4.0, 132.0), (0.0, 126.0), thin);

    // Back, head crest and snout, jaw, belly.
    curve((72.0, 112.0), (86.0, 62.0), (148.0, 46.0), (178.0, 70.0), line);
    curve((178.0, 70.0), (184.0, 50.0), (206.0, 50.0), (226.0, 84.0), line);
    curve((226.0, 84.0), (216.0, 98.0), (196.0, 104.0), (178.0, 100.0), line);
    curve((178.0, 100.0), (160.0, 126.0), (112.0, 130.0), (78.0, 122.0), line);
    // Mouth.
    curve((224.0, 86.0), (214.0, 90.0), (206.0, 90.0), (198.0, 88.0), thin);

    // Tail: a spiral curling under the branch side.
    let centre = (50.0f32, 114.0f32);
    let turns = 2.15 * std::f32::consts::PI;
    let tail: Vec<Pos2> = (0..=60)
        .map(|i| {
            let k = i as f32 / 60.0;
            let a = k * turns;
            let r = 26.0 * (1.0 - 0.82 * k);
            at(centre.0 + r * a.cos(), centre.1 + r * a.sin())
        })
        .collect();
    p.add(PathShape::line(tail, PathStroke::new(3.0 * s, green)));
    // Join the tail to the body.
    curve((76.0, 114.0), (76.0, 117.0), (77.0, 120.0), (78.0, 122.0), line);

    // Legs gripping the branch.
    for (hip, knee, foot) in [((158.0, 122.0), (170.0, 134.0), (166.0, 150.0)), ((104.0, 128.0), (92.0, 138.0), (100.0, 150.0))] {
        p.line_segment([at(hip.0, hip.1), at(knee.0, knee.1)], line);
        p.line_segment([at(knee.0, knee.1), at(foot.0, foot.1)], line);
        p.line_segment([at(foot.0 - 6.0, foot.1 + 1.0), at(foot.0 + 6.0, foot.1 - 1.0)], line);
    }

    // Engraved bands along the body.
    for (x, top, bottom) in [(108.0, 66.0, 126.0), (126.0, 58.0, 128.0), (144.0, 56.0, 126.0), (162.0, 62.0, 116.0)] {
        curve((x, top), (x + 6.0, top + 20.0), (x + 6.0, bottom - 20.0), (x - 2.0, bottom), thin);
    }

    // The eye: a turret with a pupil looking forward.
    p.circle_filled(at(196.0, 74.0), 11.0 * s, t.bg);
    p.circle_stroke(at(196.0, 74.0), 11.0 * s, line);
    p.circle_filled(at(199.0, 73.0), 4.0 * s, green);
}
