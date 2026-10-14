//! ScreenGrabCraft Editor's artwork: a barn owl, head and shoulders, face on, with a pencil held
//! crosswise in its beak.
//!
//! In the manner of a wood engraving: the heart-shaped facial disc in paper with fine lines
//! radiating from the eyes, edged by its dark ruff; the crown in lines that go round the head,
//! flecked with pale spots; the folded wings as rows of engraved feathers; the breast lightly
//! spotted; the pencil faceted, its lower face in shadow.

use super::pen::{Canvas, Curve, Part, Pt, arc, at, both, ellipse, lens, poly, without};

/// Head and shoulders in one outline: over the crown, down the right of the head, out along the
/// right shoulder, off the edges, and back along the left.
fn body() -> Curve {
    at(500.0, 90.0)
        .c(650.0, 88.0, 800.0, 180.0, 826.0, 340.0)
        .c(842.0, 440.0, 826.0, 520.0, 790.0, 580.0)
        .c(860.0, 600.0, 960.0, 640.0, 1060.0, 700.0)
        .l(1060.0, 1060.0)
        .l(-60.0, 1060.0)
        .l(-60.0, 700.0)
        .c(40.0, 640.0, 140.0, 600.0, 210.0, 580.0)
        .c(174.0, 520.0, 158.0, 440.0, 174.0, 340.0)
        .c(200.0, 180.0, 350.0, 88.0, 500.0, 90.0)
}

/// The head alone, down to under the chin.
fn head() -> Curve {
    at(500.0, 90.0)
        .c(650.0, 88.0, 800.0, 180.0, 826.0, 340.0)
        .c(842.0, 440.0, 826.0, 520.0, 790.0, 580.0)
        .c(720.0, 680.0, 600.0, 742.0, 500.0, 750.0)
        .c(400.0, 742.0, 280.0, 680.0, 210.0, 580.0)
        .c(174.0, 520.0, 158.0, 440.0, 174.0, 340.0)
        .c(200.0, 180.0, 350.0, 88.0, 500.0, 90.0)
}

/// The facial disc: a heart, its two lobes meeting over the bill, its point under the chin.
fn disc() -> Curve {
    at(500.0, 180.0)
        .c(462.0, 140.0, 402.0, 126.0, 345.0, 138.0)
        .c(262.0, 158.0, 214.0, 258.0, 214.0, 360.0)
        .c(214.0, 470.0, 300.0, 600.0, 420.0, 680.0)
        .c(452.0, 702.0, 480.0, 718.0, 500.0, 730.0)
        .c(520.0, 718.0, 548.0, 702.0, 580.0, 680.0)
        .c(700.0, 600.0, 786.0, 470.0, 786.0, 360.0)
        .c(786.0, 258.0, 738.0, 158.0, 655.0, 138.0)
        .c(598.0, 126.0, 538.0, 140.0, 500.0, 180.0)
}

/// The ridge of feathers down the middle of the face, to the bill.
fn ridge() -> Curve {
    at(484.0, 186.0).c(474.0, 270.0, 476.0, 390.0, 486.0, 468.0).l(514.0, 468.0).c(524.0, 390.0, 526.0, 270.0, 516.0, 186.0)
}

/// A folded wing, from the shoulder off the side, its inner edge coming down beside the breast;
/// the right one mirrors the left.
fn wing(left: bool) -> Curve {
    let w = at(-60.0, 690.0).c(60.0, 626.0, 170.0, 590.0, 252.0, 606.0).c(332.0, 680.0, 372.0, 860.0, 350.0, 1060.0).l(-60.0, 1060.0);
    if left { w } else { mirror(&w) }
}

fn mirror(c: &Curve) -> Curve {
    c.mapped(|(x, y)| (1000.0 - x, y))
}

const EYES: [Pt; 2] = [(392.0, 372.0), (608.0, 372.0)];

/// The pencil, held crosswise: its axis through the bill, rising a little to the right.
const HOLD: Pt = (500.0, 560.0);
const TILT: f32 = -7.0;

/// A shape written along the pencil (x along it from where the bill holds it, y across it) put
/// where the pencil is.
fn on_pencil(c: Curve) -> Curve {
    c.mapped(|(s, n)| (HOLD.0 + s, HOLD.1 + n)).rotated(HOLD, TILT)
}

pub fn draw(cv: &mut Canvas) -> Result<(), String> {
    let body = body();
    let all = cv.mask(&[&body])?;
    cv.fill(&body, Part::Paper, None);
    let head_m = cv.mask(&[&head()])?;
    let disc_m = cv.mask(&[&disc()])?;
    let band = without(&head_m, &disc_m);

    // ---- the wings: rows of feathers hanging outwards, each with its veins and a fleck ----
    for left in [true, false] {
        let clip = without(&both(&cv.mask(&[&wing(left)])?, &all), &head_m);
        let (pivot, turn) = if left { ((150.0, 820.0), 30.0) } else { ((850.0, 820.0), -30.0) };
        // The buff of the wing: fine lines with the grain of the feathers.
        cv.hatch((if left { -0.5 } else { 0.5 }, 0.866), 11.0, 2.8, &clip);
        let mut v = 520.0;
        let mut row = 0;
        while v < 1300.0 {
            let s = 24.0 + 0.07 * (v - 520.0);
            let mut u = -300.0 + if row % 2 == 1 { s } else { 0.0 };
            while u < 1300.0 {
                let place = |c: Curve| c.rotated(pivot, turn);
                let tongue = place(at(u - s, v).c(u - s, v + 1.5 * s, u + s, v + 1.5 * s, u + s, v));
                cv.stroke(&tongue, 5.0, Part::Ink, Some(&clip));
                for k in 1..4 {
                    let inset = s * (0.18 + 0.2 * k as f32);
                    let vein = place(at(u + s - inset, v + 0.2 * s).c(
                        u + s - inset,
                        v + 0.9 * s,
                        u + 0.4 * s - inset * 0.4,
                        v + 1.18 * s,
                        u,
                        v + 1.14 * s - inset * 0.3,
                    ));
                    cv.stroke(&vein, 3.0, Part::Ink, Some(&clip));
                }
                cv.fill(&place(lens((u - 0.3 * s, v + 0.62 * s), (u - 0.3 * s, v + 0.92 * s), 0.2 * s)), Part::Ink, Some(&clip));
                u += 2.0 * s;
            }
            v += s * 1.12;
            row += 1;
        }
    }

    // ---- the breast: a few small spots, and lines round its sides ----
    let breast = without(&without(&without(&all, &head_m), &cv.mask(&[&wing(true)])?), &cv.mask(&[&wing(false)])?);
    for (i, y) in (0..).zip((780..1100).step_by(48)) {
        let mut x = 280.0 + if i % 2 == 1 { 30.0 } else { 0.0 };
        let mut j = 0.0f32;
        while x < 740.0 {
            // Scattered, not set out on a grid.
            let (jx, jy) = ((j * 2.1 + i as f32 * 0.7).sin() * 14.0, (j * 1.3 + i as f32 * 2.3).cos() * 12.0);
            let (px, py) = (x + jx, y as f32 + jy);
            cv.fill(&lens((px, py), (px + 2.0, py + 14.0 + 4.0 * (j * 0.9).sin()), 9.0), Part::Ink, Some(&breast));
            x += 60.0;
            j += 1.0;
        }
    }
    // Its sides turning away under the wings: lines along the wings' edges, shorter further in.
    for k in 1..7 {
        let d = 10.0 * k as f32;
        let edge = at(252.0 + d, 606.0).c(332.0 + d, 680.0, 372.0 + d, 860.0, 350.0 + d, 1060.0).part(0.18 + 0.07 * k as f32, 1.0);
        cv.stroke(&edge, 3.0, Part::Ink, Some(&breast));
        cv.stroke(&mirror(&edge), 3.0, Part::Ink, Some(&breast));
    }

    // ---- the crown: lines round the head, flecked with pale spots ----
    for k in 0..12 {
        let r = 250.0 + 8.5 * k as f32;
        cv.stroke(&arc((500.0, 440.0), r, 150.0, 390.0), 3.8, Part::Ink, Some(&band));
    }
    for (i, r) in (0..).zip([266.0f32, 296.0, 326.0]) {
        let mut a = 160.0 + 6.0 * i as f32;
        while a < 380.0 {
            let (s, c) = a.to_radians().sin_cos();
            cv.fill(&ellipse((500.0 + r * c, 440.0 + r * s), 7.5, 5.0, a + 90.0), Part::Paper, Some(&band));
            a += 12.0;
        }
    }

    // ---- the facial disc: fine lines out from each eye, the ridge, the dark before the eyes ----
    let ridge = ridge();
    let face = without(&disc_m, &cv.mask(&[&ridge])?);
    for (i, eye) in EYES.iter().enumerate() {
        let side = if i == 0 {
            poly(&[(0.0, 0.0), (500.0, 0.0), (500.0, 1000.0), (0.0, 1000.0)])
        } else {
            poly(&[(500.0, 0.0), (1000.0, 0.0), (1000.0, 1000.0), (500.0, 1000.0)])
        };
        let clip = both(&face, &cv.mask(&[&side])?);
        // Cut with a burin: each line pointed at both ends, long and short in turn.
        let mut a = 0.0f32;
        let mut j = 0;
        while a < 360.0 {
            let (s, c) = a.to_radians().sin_cos();
            let r0 = 86.0 + [0.0, 14.0, 6.0][j % 3];
            let r1 = r0 + [150.0, 96.0, 128.0][j % 3];
            cv.fill(&lens((eye.0 + r0 * c, eye.1 + r0 * s), (eye.0 + r1 * c, eye.1 + r1 * s), 2.8), Part::Ink, Some(&clip));
            a += 7.0;
            j += 1;
        }
    }
    for edge in [at(486.0, 190.0).c(476.0, 270.0, 478.0, 390.0, 488.0, 466.0), at(514.0, 190.0).c(524.0, 270.0, 522.0, 390.0, 512.0, 466.0)] {
        cv.stroke(&edge, 3.4, Part::Ink, Some(&disc_m));
    }
    for k in 0..9 {
        let y = 214.0 + 28.0 * k as f32;
        cv.stroke(&at(489.0, y).l(500.0, y + 12.0).l(511.0, y), 2.6, Part::Ink, Some(&disc_m));
    }
    // The dark round each eye, deepest towards the bill.
    for (i, eye) in EYES.iter().enumerate() {
        let toward = if i == 0 { 1.0 } else { -1.0 };
        let surround = ellipse((eye.0 + 14.0 * toward, eye.1 + 2.0), 72.0, 60.0, -16.0 * toward);
        let clip = both(&cv.mask(&[&surround])?, &disc_m);
        let mut a = 0.0f32;
        while a < 360.0 {
            let (s, c) = a.to_radians().sin_cos();
            cv.stroke(&at(eye.0 + 40.0 * c, eye.1 + 36.0 * s).l(eye.0 + 90.0 * c, eye.1 + 80.0 * s), 3.6, Part::Ink, Some(&clip));
            a += 5.0;
        }
    }

    // ---- the ruff round the disc ----
    let rim = disc();
    let n = 150;
    for k in 0..n {
        let t = k as f32 / n as f32;
        let (p, q) = (rim.point(t), rim.point(t + 0.002));
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let d = (dx * dx + dy * dy).sqrt().max(1e-3);
        let mut out = (-dy / d, dx / d);
        if out.0 * (p.0 - 500.0) + out.1 * (p.1 - 420.0) < 0.0 {
            out = (-out.0, -out.1);
        }
        cv.stroke(&at(p.0, p.1).l(p.0 + out.0 * 24.0, p.1 + out.1 * 24.0), 4.0, Part::Ink, Some(&head_m));
    }
    cv.stroke(&rim, 15.0, Part::Ink, None);

    // ---- the eyes ----
    for (i, eye) in EYES.iter().enumerate() {
        let tilt = if i == 0 { 10.0 } else { -10.0 };
        cv.fill(&ellipse(*eye, 46.0, 41.0, tilt), Part::Ink, None);
        cv.fill(&ellipse((eye.0 - 14.0, eye.1 - 13.0), 11.0, 10.0, 0.0), Part::Paper, None);
    }

    // ---- outlines ----
    cv.stroke(&body, 11.5, Part::Ink, None);
    cv.stroke(&head().part(0.32, 0.68), 9.0, Part::Ink, None);
    for left in [true, false] {
        let w = wing(left);
        cv.stroke(&w.part(0.0, 0.62), 9.0, Part::Ink, Some(&without(&all, &head_m)));
    }

    // ---- the pencil, and its shadow on the face below it ----
    let mut n = 38.0;
    let mut k = 0.0;
    while n < 78.0 {
        let shadow = on_pencil(at(-230.0 + 30.0 * k, n).l(300.0 - 40.0 * k, n));
        cv.stroke(&shadow, 3.0, Part::Ink, Some(&disc_m));
        n += 7.0;
        k += 1.0;
    }
    pencil(cv)?;

    // ---- the bill, closed over the pencil ----
    let bill = at(480.0, 452.0)
        .c(486.0, 440.0, 514.0, 440.0, 520.0, 452.0)
        .c(526.0, 490.0, 516.0, 530.0, 500.0, 556.0)
        .c(484.0, 530.0, 474.0, 490.0, 480.0, 452.0);
    cv.fill(&bill, Part::Paper, None);
    let shade = cv.mask(&[&bill])?;
    for k in 0..3 {
        let d = 5.0 * k as f32;
        cv.stroke(&at(508.0 + d, 462.0).c(512.0 + d, 490.0, 508.0 + d * 0.5, 520.0, 500.0 + d * 0.3, 548.0), 2.6, Part::Ink, Some(&shade));
    }
    cv.stroke(&bill, 6.5, Part::Ink, None);
    Ok(())
}

/// The pencil, in its own frame: x along it from the bill (the point to the left), y across it.
fn pencil(cv: &mut Canvas) -> Result<(), String> {
    let (half, facet) = (33.0, 11.0);
    let (point, wood, paint, ferrule, rubber, end) = (-430.0, -395.0, -306.0, 330.0, 394.0, 436.0);
    // The painted body, its sharpened end scalloped where the knife went round each face.
    let body = on_pencil(
        at(paint, -half)
            .l(ferrule, -half)
            .l(ferrule, half)
            .l(paint, half)
            .c(paint - 12.0, half - 4.0, paint - 12.0, facet + 4.0, paint, facet)
            .c(paint - 14.0, facet - 4.0, paint - 14.0, -facet + 4.0, paint, -facet)
            .c(paint - 12.0, -facet - 4.0, paint - 12.0, -half + 4.0, paint, -half),
    );
    let cone = on_pencil(poly(&[(paint - 6.0, -half), (wood, -9.0), (wood, 9.0), (paint - 6.0, half)]));
    let lead = on_pencil(poly(&[(wood, -9.0), (point, 0.0), (wood, 9.0)]));
    let band = on_pencil(poly(&[(ferrule, -half - 2.0), (rubber, -half - 2.0), (rubber, half + 2.0), (ferrule, half + 2.0)]));
    let tip = on_pencil(
        at(rubber, -half + 2.0).l(end - 16.0, -half + 2.0).c(end, -half + 2.0, end, half - 2.0, end - 16.0, half - 2.0).l(rubber, half - 2.0),
    );

    cv.fill(&cone, Part::Paper, None);
    let cone_m = cv.mask(&[&cone])?;
    for n in [-20.0, -6.0, 8.0, 22.0] {
        cv.stroke(&on_pencil(at(paint - 4.0, n).l(wood + 4.0, n * 0.3)), 2.4, Part::Ink, Some(&cone_m));
    }
    cv.stroke(&cone, 6.0, Part::Ink, None);
    cv.fill(&lead, Part::Ink, None);

    cv.fill(&body, Part::Paper, None);
    let body_m = cv.mask(&[&body])?;
    // Light from above: the top face bare, the middle one in lines along it, the lower one dark.
    let middle = both(&body_m, &cv.mask(&[&on_pencil(poly(&[(paint - 40.0, -facet), (ferrule, -facet), (ferrule, facet), (paint - 40.0, facet)]))])?);
    let mut n = -facet + 5.0;
    while n < facet {
        cv.stroke(&on_pencil(at(paint - 40.0, n).l(ferrule, n)), 2.8, Part::Ink, Some(&middle));
        n += 6.0;
    }
    let lower =
        both(&body_m, &cv.mask(&[&on_pencil(poly(&[(paint - 40.0, facet), (ferrule, facet), (ferrule, half + 4.0), (paint - 40.0, half + 4.0)]))])?);
    cv.cover(&lower, Part::Ink);
    for n in [-facet, facet] {
        cv.stroke(&on_pencil(at(paint, n).l(ferrule, n)), 3.4, Part::Ink, Some(&body_m));
    }
    cv.stroke(&body, 7.0, Part::Ink, None);

    cv.fill(&tip, Part::Paper, None);
    let tip_m = cv.mask(&[&tip])?;
    let mut n = 8.0;
    while n < half {
        cv.stroke(&on_pencil(at(rubber, n).l(end, n)), 2.6, Part::Ink, Some(&tip_m));
        n += 6.5;
    }
    cv.stroke(&tip, 7.0, Part::Ink, None);
    cv.fill(&band, Part::Paper, None);
    let band_m = cv.mask(&[&band])?;
    for s in [ferrule + 10.0, ferrule + 22.0, rubber - 22.0, rubber - 10.0] {
        cv.stroke(&on_pencil(at(s, -half - 4.0).l(s, half + 4.0)), 5.0, Part::Ink, Some(&band_m));
    }
    let mut n = 6.0;
    while n < half + 2.0 {
        cv.stroke(&on_pencil(at(ferrule + 26.0, n).l(rubber - 26.0, n)), 2.4, Part::Ink, Some(&band_m));
        n += 6.0;
    }
    cv.stroke(&band, 7.0, Part::Ink, None);
    Ok(())
}
