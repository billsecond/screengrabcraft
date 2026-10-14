//! ScreenGrabCraft's artwork: a peregrine falcon, head and shoulders, in three-quarter view
//! facing left, framed by the four corner brackets of a capture selection.
//!
//! In the manner of a wood engraving: confident ink outlines; the dark hood (the crown, the face
//! round the eye and the broad "moustache" below it) cut with white lines that go round the
//! skull; the pale breast barred with rows of engraved marks that follow its swell; the back and
//! the folded wing as rows of feathers over lines that run with their grain.

use super::pen::{Canvas, Curve, Part, Pt, arc, at, both, circle, lens, poly, without};

/// Head, neck and body in one outline: from above the cere, over the crown, down the nape and the
/// back, off the right and bottom edges, up the breast and the throat to the chin.
fn body() -> Curve {
    at(318.0, 300.0)
        .c(335.0, 215.0, 405.0, 160.0, 490.0, 157.0)
        .c(585.0, 154.0, 655.0, 205.0, 676.0, 285.0)
        .c(692.0, 345.0, 700.0, 400.0, 738.0, 448.0)
        .c(800.0, 520.0, 930.0, 545.0, 1060.0, 565.0)
        .l(1060.0, 1060.0)
        .l(205.0, 1060.0)
        .c(178.0, 950.0, 168.0, 830.0, 190.0, 730.0)
        .c(212.0, 630.0, 280.0, 545.0, 318.0, 488.0)
        .c(334.0, 462.0, 338.0, 432.0, 330.0, 405.0)
}

/// The dark hood: the crown and nape, the face round the eye, and the moustache.
fn hood() -> Curve {
    at(318.0, 300.0)
        .c(335.0, 215.0, 405.0, 160.0, 490.0, 157.0)
        .c(585.0, 154.0, 655.0, 205.0, 676.0, 285.0)
        .c(692.0, 345.0, 700.0, 400.0, 738.0, 448.0)
        .l(760.0, 470.0)
        // Forward along the top of the pale cheek, to behind the eye.
        .c(700.0, 455.0, 600.0, 395.0, 478.0, 352.0)
        // Down the back of the moustache, round its tip and up its front to the gape.
        .c(470.0, 420.0, 452.0, 490.0, 420.0, 552.0)
        .c(410.0, 570.0, 390.0, 570.0, 384.0, 552.0)
        .c(372.0, 495.0, 356.0, 430.0, 346.0, 375.0)
        .l(318.0, 368.0)
}

/// The mantle: the back between the nape and the wing.
fn mantle() -> Curve {
    at(745.0, 440.0).l(1060.0, 440.0).l(1060.0, 640.0).l(640.0, 520.0).c(660.0, 490.0, 700.0, 460.0, 745.0, 440.0)
}

/// The folded wing, its leading edge coming down across the flank.
fn wing() -> Curve {
    at(640.0, 520.0).c(720.0, 500.0, 900.0, 540.0, 1060.0, 600.0).l(1060.0, 1060.0).l(540.0, 1060.0).c(505.0, 930.0, 520.0, 720.0, 640.0, 520.0)
}

fn wing_edge() -> Curve {
    at(540.0, 1060.0).c(505.0, 930.0, 520.0, 720.0, 640.0, 520.0).c(720.0, 500.0, 900.0, 540.0, 1060.0, 600.0)
}

const EYE: Pt = (404.0, 292.0);

/// The upper mandible: down the curve of the culmen from the cere to the hook, back under it
/// and along the cutting edge, with the falcon's notch, to the gape.
fn beak_upper() -> Curve {
    at(304.0, 300.0)
        .c(262.0, 296.0, 222.0, 318.0, 208.0, 356.0)
        .c(198.0, 384.0, 202.0, 412.0, 218.0, 436.0)
        .c(222.0, 414.0, 230.0, 400.0, 244.0, 393.0)
        .l(258.0, 388.0)
        .l(264.0, 396.0)
        .c(280.0, 386.0, 296.0, 380.0, 318.0, 378.0)
        .l(318.0, 300.0)
}

fn beak_lower() -> Curve {
    at(252.0, 398.0).c(270.0, 410.0, 300.0, 412.0, 326.0, 402.0).l(340.0, 384.0).l(310.0, 380.0).c(292.0, 384.0, 276.0, 390.0, 262.0, 397.0)
}

/// The cere: the band of bare skin at the root of the beak.
fn cere() -> Curve {
    at(330.0, 294.0)
        .c(314.0, 290.0, 304.0, 296.0, 300.0, 306.0)
        .c(294.0, 330.0, 298.0, 356.0, 310.0, 378.0)
        .l(338.0, 378.0)
        .c(338.0, 350.0, 336.0, 318.0, 330.0, 294.0)
}

/// One corner bracket of the selection: its outer corner at `corner`, its arms running `dx` and `dy`.
fn bracket(corner: Pt, dx: f32, dy: f32) -> Curve {
    let (x, y) = corner;
    let (len, t) = (128.0, 40.0);
    poly(&[(x, y), (x + dx * len, y), (x + dx * len, y + dy * t), (x + dx * t, y + dy * t), (x + dx * t, y + dy * len), (x, y + dy * len), (x, y)])
}

pub fn draw(cv: &mut Canvas) -> Result<(), String> {
    let body = body();
    let all = cv.mask(&[&body])?;
    cv.fill(&body, Part::Paper, None);
    let hood_m = both(&cv.mask(&[&hood()])?, &all);
    let wing_m = both(&cv.mask(&[&wing()])?, &all);
    let mantle_m = without(&without(&both(&cv.mask(&[&mantle()])?, &all), &hood_m), &wing_m);
    let front = without(&without(&without(&all, &hood_m), &wing_m), &mantle_m);

    // ---- the breast: drops high up, bars below, in rows that sag round it ----
    for (i, y) in (0..).zip((630..1120).step_by(28)) {
        let y = y as f32;
        let sag = 24.0 + 0.06 * (y - 630.0);
        let row = at(150.0, y - 22.0).c(300.0, y + sag, 480.0, y + sag, 680.0, y - 30.0);
        let grow = ((y - 660.0) / 300.0).clamp(0.0, 1.0);
        let (dash, gap) = (0.022 + 0.04 * grow, 0.05 - 0.018 * grow);
        let mut t = if i % 2 == 1 { (dash + gap) / 2.0 } else { 0.0 };
        let mut j = 0.0f32;
        while t < 1.0 {
            // A hand, not a machine: each mark a little longer or shorter than the last.
            let wobble = (j * 2.39 + i as f32 * 1.7).sin();
            let (a, b) = (row.point(t), row.point(t + dash * (1.0 + 0.22 * wobble)));
            cv.fill(&lens(a, b, (7.0 + 5.0 * grow) * (1.0 + 0.12 * wobble)), Part::Ink, Some(&front));
            t += dash + gap;
            j += 1.0;
        }
    }
    // Its swell: lines along the left of it.
    for k in 0..5 {
        let r = 336.0 + 14.0 * k as f32;
        cv.stroke(&arc((560.0, 830.0), r, 154.0 + 4.0 * k as f32, 210.0 - 3.0 * k as f32), 3.4, Part::Ink, Some(&front));
    }
    // The dark under the wing's leading edge.
    for k in 0..8 {
        let d = 10.0 * k as f32;
        let line = at(566.0 - d * 0.7, 630.0 + d).c(522.0 - d, 760.0, 515.0 - d, 920.0, 548.0 - d, 1060.0);
        cv.stroke(&line, 3.8, Part::Ink, Some(&front));
    }
    // Under the chin, the shadow of the head.
    for k in 0..4 {
        let d = 11.0 * k as f32;
        cv.stroke(&at(318.0 + d * 0.2, 420.0 + d).c(330.0, 424.0 + d, 340.0, 426.0 + d, 352.0, 420.0 + d), 3.2, Part::Ink, Some(&front));
    }
    // The side of the neck turning away: bands that sag round it, from the back, each a little
    // shorter than the one below.
    for (i, y) in (0..).zip((430..640).step_by(15)) {
        let y = y as f32;
        let band = at(800.0, y - 56.0).c(700.0, y + 4.0, 600.0, y + 34.0, 470.0, y + 30.0);
        let reach = 0.52 + 0.018 * i as f32 + if i % 2 == 1 { 0.06 } else { 0.0 };
        cv.stroke(&band.part(0.0, reach), 3.2, Part::Ink, Some(&front));
    }

    // ---- the mantle and the wing: lines with the grain, then the feathers over them ----
    let grain = (0.559, 0.829);
    let back = cv.mask(&[&wing(), &mantle()])?;
    cv.hatch(grain, 7.5, 2.7, &both(&both(&back, &all), &without(&all, &hood_m)));
    let feathers = |cv: &mut Canvas, clip: &tiny_skia::Mask, top: f32, bottom: f32, size: f32, grow: f32, pivot: Pt, line: f32| {
        let mut v = top;
        let mut row = 0;
        while v < bottom {
            let s = size + grow * (v - top);
            let mut u = 200.0 + if row % 2 == 1 { s } else { 0.0 };
            while u < 1500.0 {
                let turn = |c: Curve| c.rotated(pivot, -34.0);
                let tongue = turn(at(u - s, v).c(u - s, v + 1.5 * s, u + s, v + 1.5 * s, u + s, v));
                // A pale fringe inside the edge, then the edge.
                let fringe = turn(at(u - 0.8 * s, v + 0.1 * s).c(u - 0.8 * s, v + 1.28 * s, u + 0.8 * s, v + 1.28 * s, u + 0.8 * s, v + 0.1 * s));
                cv.stroke(&fringe, 0.17 * s, Part::Paper, Some(clip));
                cv.stroke(&tongue, line, Part::Ink, Some(clip));
                u += 2.0 * s;
            }
            v += s * 1.1;
            row += 1;
        }
    };
    feathers(cv, &mantle_m, 380.0, 760.0, 17.0, 0.05, (850.0, 560.0), 4.0);
    feathers(cv, &wing_m, 500.0, 1500.0, 30.0, 0.085, (800.0, 760.0), 5.4);

    // ---- the hood, cut with white lines round the skull ----
    cv.cover(&hood_m, Part::Ink);
    let face = cv.mask(&[&circle(EYE, 80.0)])?;
    let lines = without(&hood_m, &face);
    for k in 0..11 {
        let r = 90.0 + 17.0 * k as f32;
        let line = arc((482.0, 332.0), r, 242.0 + 3.0 * k as f32, 380.0 - 2.0 * k as f32);
        cv.stroke(&line, 2.4, Part::Paper, Some(&lines));
    }

    // ---- outlines ----
    cv.stroke(&body, 11.5, Part::Ink, None);
    cv.stroke(&wing_edge(), 9.0, Part::Ink, None);

    // ---- the beak (dark, lit along the top), the cere and the eye ----
    let lower = beak_lower();
    cv.fill(&lower, Part::Ink, None);
    let upper = beak_upper();
    cv.fill(&upper, Part::Ink, None);
    cv.stroke(&at(262.0, 399.0).c(284.0, 390.0, 306.0, 385.0, 336.0, 384.0), 3.6, Part::Paper, None);
    let lit = cv.mask(&[&upper])?;
    for k in 0..2 {
        let d = 10.0 * k as f32;
        let line = at(300.0, 312.0 + d).c(266.0, 310.0 + d, 236.0, 326.0 + d, 224.0, 356.0 + d * 0.5);
        cv.stroke(&line.part(0.0, 0.9 - 0.25 * k as f32), 3.4, Part::Paper, Some(&lit));
    }
    let cere = cere();
    cv.fill(&cere, Part::Paper, None);
    cv.stroke(&cere, 6.0, Part::Ink, None);
    cv.fill(&circle((318.0, 322.0), 6.5), Part::Ink, None);

    cv.fill(&circle(EYE, 57.0), Part::Paper, None);
    cv.stroke(&circle(EYE, 57.0), 6.0, Part::Ink, None);
    cv.fill(&circle(EYE, 40.0), Part::Ink, None);
    cv.fill(&circle((EYE.0 - 13.0, EYE.1 - 9.0), 10.0), Part::Paper, None);
    // The brow overhangs the eye.
    let brow = at(330.0, 268.0).c(366.0, 222.0, 440.0, 214.0, 488.0, 244.0).c(450.0, 248.0, 400.0, 258.0, 338.0, 294.0);
    cv.fill(&brow, Part::Ink, None);

    // ---- the selection's corners ----
    for (corner, dx, dy) in [((165.0, 125.0), 1.0, 1.0), ((775.0, 125.0), -1.0, 1.0), ((165.0, 645.0), 1.0, -1.0), ((775.0, 645.0), -1.0, -1.0)] {
        let b = bracket(corner, dx, dy);
        cv.fill(&b, Part::Paper, None);
        cv.stroke(&b, 8.0, Part::Ink, None);
    }
    Ok(())
}
