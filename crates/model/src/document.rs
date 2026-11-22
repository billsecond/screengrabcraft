//! A picture being worked on: the captured (or opened) picture, the annotations on it in the
//! order they are drawn, and the pictures placed on it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::{Pt, Rect};
use crate::object::{Kind, Object, ObjectId, Shape, StepLabels, Style};
use crate::raster::Raster;

/// The most annotations one document holds.
pub const MAX_OBJECTS: usize = 10_000;
/// The most pictures placed on one document.
pub const MAX_IMAGES: usize = 256;

/// Where a capture came from and when, kept with the document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Meta {
    /// When it was captured (RFC 3339 text, local time).
    pub captured: Option<String>,
    /// What it is of: a window's title, a monitor's name.
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub base: Raster,
    /// Bottom first.
    pub objects: Vec<Object>,
    /// Pictures placed on the picture, by key.
    pub images: BTreeMap<u64, Raster>,
    pub meta: Meta,
    next_id: u64,
}

/// Where to move annotations in the drawing order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Order {
    Front,
    Back,
    Forward,
    Backward,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DocError {
    #[error("there is no annotation with id {0}")]
    NoObject(u64),
    #[error("a document holds at most {MAX_OBJECTS} annotations")]
    TooManyObjects,
    #[error("a document holds at most {MAX_IMAGES} placed pictures")]
    TooManyImages,
    #[error("there is no placed picture with key {0}")]
    NoImage(u64),
}

/// The furthest past its edges a picture grows when things are moved off them.
pub const MAX_GROW: f32 = 4096.0;

impl Document {
    pub fn new(base: Raster) -> Document {
        Document { base, objects: Vec::new(), images: BTreeMap::new(), meta: Meta::default(), next_id: 1 }
    }

    pub fn width(&self) -> u32 {
        self.base.width()
    }

    pub fn height(&self) -> u32 {
        self.base.height()
    }

    /// The picture's area.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width() as f32, self.height() as f32)
    }

    /// Adds an annotation on top and returns its id.
    pub fn add(&mut self, shape: Shape, style: Style) -> Result<ObjectId, DocError> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err(DocError::TooManyObjects);
        }
        if let Shape::Image { image, .. } = &shape
            && !self.images.contains_key(image)
        {
            return Err(DocError::NoImage(*image));
        }
        let id = ObjectId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.objects.push(Object::new(id, shape.sanitized(style.width), style.clamped()));
        Ok(id)
    }

    /// Adds a whole object read from outside: a fresh id when its own is taken.
    pub fn insert(&mut self, mut object: Object) -> Result<ObjectId, DocError> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err(DocError::TooManyObjects);
        }
        if let Shape::Image { image, .. } = &object.shape
            && !self.images.contains_key(image)
        {
            return Err(DocError::NoImage(*image));
        }
        if object.id.0 == 0 || self.get(object.id).is_some() {
            object.id = ObjectId(self.next_id);
        }
        self.next_id = self.next_id.max(object.id.0.saturating_add(1));
        object.shape = object.shape.sanitized(object.style.width);
        object.style = object.style.clamped();
        object.rotation = crate::object::clamp_rotation(object.rotation);
        let id = object.id;
        self.objects.push(object);
        Ok(id)
    }

    /// Keeps a placed picture and returns its key.
    pub fn add_image(&mut self, raster: Raster) -> Result<u64, DocError> {
        if self.images.len() >= MAX_IMAGES {
            return Err(DocError::TooManyImages);
        }
        let key = self.images.keys().next_back().map_or(1, |k| k.saturating_add(1));
        self.images.insert(key, raster);
        Ok(key)
    }

    /// Placed pictures no annotation uses any more are let go.
    pub fn prune_images(&mut self) {
        let used: std::collections::BTreeSet<u64> = self
            .objects
            .iter()
            .filter_map(|o| match o.shape {
                Shape::Image { image, .. } => Some(image),
                _ => None,
            })
            .collect();
        self.images.retain(|k, _| used.contains(k));
    }

    pub fn get(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn index_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.iter().position(|o| o.id == id)
    }

    /// Removes the annotations with these ids; returns how many there were.
    pub fn remove(&mut self, ids: &[ObjectId]) -> usize {
        let before = self.objects.len();
        self.objects.retain(|o| !ids.contains(&o.id));
        self.prune_images();
        before - self.objects.len()
    }

    /// The topmost annotation under `p`.
    pub fn object_at(&self, p: Pt, tol: f32) -> Option<ObjectId> {
        self.objects.iter().rev().find(|o| o.hit(p, tol)).map(|o| o.id)
    }

    /// Annotations whose bounds lie inside `area` (a rubber-band selection).
    pub fn objects_in(&self, area: Rect) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|o| {
                let b = o.bounds();
                area.contains(b.min()) && area.contains(b.max())
            })
            .map(|o| o.id)
            .collect()
    }

    /// `d` cut down so that these annotations, moved by it together, stay on the picture.
    pub fn limit_move(&self, ids: &[ObjectId], d: Pt) -> Pt {
        let area = ids.iter().filter_map(|id| self.get(*id)).map(Object::bounds).reduce(Rect::union);
        area.map_or(d, |a| a.limit_move(d, self.bounds()))
    }

    /// `d` cut down so that these annotations, moved by it together, stay within `reach` of the
    /// picture (they may go off it: the picture grows to hold them, see [`Document::grow_to`]).
    pub fn limit_move_near(&self, ids: &[ObjectId], d: Pt, reach: f32) -> Pt {
        let b = self.bounds();
        let reach = if reach.is_finite() { reach.clamp(0.0, MAX_GROW) } else { 0.0 };
        let wide = Rect::new(b.x - reach, b.y - reach, b.w + reach * 2.0, b.h + reach * 2.0);
        self.bounds_of(ids).map_or(d, |a| a.limit_move(d, wide))
    }

    /// The box round these annotations together.
    pub fn bounds_of(&self, ids: &[ObjectId]) -> Option<Rect> {
        ids.iter().filter_map(|id| self.get(*id)).map(Object::bounds).reduce(Rect::union)
    }

    /// Makes the picture larger, see-through where it is new, so that `area` is on it. What was
    /// on it keeps its place in the picture, so everything moves along if it grew to the left or
    /// upwards: by how far. None when it need not grow, or would be too large to.
    pub fn grow_to(&mut self, area: Rect) -> Option<Pt> {
        if ![area.x, area.y, area.w, area.h].iter().all(|v| v.is_finite()) {
            return None;
        }
        let (w, h) = (self.width(), self.height());
        let side = |v: f32| v.max(0.0).ceil().min(MAX_GROW * 2.0) as u32;
        let (l, t) = (side(-area.x), side(-area.y));
        let (r, d) = (side(area.right() - w as f32), side(area.bottom() - h as f32));
        if l + t + r + d == 0 {
            return None;
        }
        let (nw, nh) = (w.checked_add(l)?.checked_add(r)?, h.checked_add(t)?.checked_add(d)?);
        let mut grown = Raster::filled(nw, nh, crate::Color::TRANSPARENT).ok()?;
        let (row, new_row) = (w as usize * 4, nw as usize * 4);
        let px = grown.pixels_mut();
        for (y, line) in self.base.pixels().chunks_exact(row.max(1)).enumerate() {
            let at = (y + t as usize) * new_row + l as usize * 4;
            if let Some(dst) = px.get_mut(at..at + row) {
                dst.copy_from_slice(line);
            }
        }
        self.base = grown;
        let shift = Pt::new(l as f32, t as f32);
        if l > 0 || t > 0 {
            self.map_objects(&|p| p.add(shift), 1.0);
        }
        Some(shift)
    }

    /// Cuts the picture back to `keep` (a part of it) and whatever its annotations take up
    /// beyond that: growth that turned out not to be needed goes. How far everything moved
    /// (back to the left, or up); none when nothing was cut.
    pub fn shrink_to(&mut self, keep: Rect) -> Option<Pt> {
        let (w, h) = (self.width(), self.height());
        let need = self.objects.iter().map(Object::bounds).fold(keep, Rect::union);
        if ![need.x, need.y, need.w, need.h].iter().all(|v| v.is_finite()) {
            return None;
        }
        let (x0, y0) = (need.x.floor().clamp(0.0, w as f32) as u32, need.y.floor().clamp(0.0, h as f32) as u32);
        let (x1, y1) = (need.right().ceil().clamp(0.0, w as f32) as u32, need.bottom().ceil().clamp(0.0, h as f32) as u32);
        if (x0, y0, x1, y1) == (0, 0, w, h) || x1 <= x0 || y1 <= y0 {
            return None;
        }
        self.base = self.base.crop(x0, y0, x1 - x0, y1 - y0)?;
        let shift = Pt::new(-(x0 as f32), -(y0 as f32));
        if x0 > 0 || y0 > 0 {
            self.map_objects(&|p| p.add(shift), 1.0);
        }
        Some(shift)
    }

    /// Moves an annotation onto the picture, as little as it takes.
    pub fn bring_inside(&mut self, id: ObjectId) {
        let bounds = self.bounds();
        if let Some(o) = self.get_mut(id) {
            let d = o.bounds().way_into(bounds);
            o.shape.translate(d);
        }
    }

    /// The number the next step badge written with `labels` gets: one more than the highest so
    /// far of those written the same way (numbers, letters and Roman numerals each count on
    /// their own, so 1, 2, 3 can have a, b, c beside them).
    pub fn next_step_number(&self, labels: StepLabels) -> u32 {
        self.objects
            .iter()
            .filter_map(|o| match o.shape {
                Shape::Step { number, labels: l, .. } if l == labels => Some(number),
                _ => None,
            })
            .max()
            .map_or(1, |n| n.saturating_add(1))
    }

    /// Moves annotations in the drawing order, keeping their order among themselves.
    pub fn reorder(&mut self, ids: &[ObjectId], order: Order) {
        let picked = |o: &Object| ids.contains(&o.id);
        match order {
            Order::Front | Order::Back => {
                let (mut moving, rest): (Vec<Object>, Vec<Object>) = self.objects.drain(..).partition(picked);
                self.objects = if order == Order::Front {
                    let mut all = rest;
                    all.append(&mut moving);
                    all
                } else {
                    moving.extend(rest);
                    moving
                };
            }
            Order::Forward => {
                for i in (0..self.objects.len().saturating_sub(1)).rev() {
                    let (a, b) = (self.objects.get(i).is_some_and(picked), self.objects.get(i + 1).is_some_and(picked));
                    if a && !b {
                        self.objects.swap(i, i + 1);
                    }
                }
            }
            Order::Backward => {
                for i in 1..self.objects.len() {
                    let (a, b) = (self.objects.get(i - 1).is_some_and(picked), self.objects.get(i).is_some_and(picked));
                    if b && !a {
                        self.objects.swap(i - 1, i);
                    }
                }
            }
        }
    }

    /// Every annotation's geometry passed through `f`, and its lengths scaled by `scale` (for a
    /// picture that was cropped, resized, turned or flipped).
    pub fn map_objects(&mut self, f: &dyn Fn(Pt) -> Pt, scale: f32) {
        // A mirror (a flip) turns a turned box the other way.
        let (o0, ox, oy) = (f(Pt::ZERO), f(Pt::new(1.0, 0.0)), f(Pt::new(0.0, 1.0)));
        let (a, b) = (ox.sub(o0), oy.sub(o0));
        let mirrors = a.x * b.y - a.y * b.x < 0.0;
        for o in &mut self.objects {
            o.shape.map_points(f);
            if mirrors {
                o.rotation = crate::object::clamp_rotation(-o.rotation);
            }
            if (scale - 1.0).abs() > f32::EPSILON {
                o.style = o.style.scaled(scale);
                if let Shape::Spotlight { .. } = o.shape {
                    // Spotlight strength is a darkness, not a length.
                } else if matches!(o.shape.kind(), Kind::Blur | Kind::Pixelate) {
                    o.style.strength = (o.style.strength * scale).clamp(1.0, 200.0);
                }
            }
        }
    }

    /// How many annotations of each kind it has (for summaries).
    pub fn count(&self, kind: Kind) -> usize {
        self.objects.iter().filter(|o| o.shape.kind() == kind).count()
    }

    /// These annotations and every other in a group with one of them, in drawing order.
    pub fn with_groups(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let groups: Vec<u64> = ids.iter().filter_map(|id| self.get(*id)).filter_map(|o| o.group).collect();
        self.objects.iter().filter(|o| ids.contains(&o.id) || o.group.is_some_and(|g| groups.contains(&g))).map(|o| o.id).collect()
    }

    /// A group number no annotation has.
    pub fn next_group(&self) -> u64 {
        self.objects.iter().filter_map(|o| o.group).max().map_or(1, |g| g.saturating_add(1))
    }

    /// These annotations as the units that move together: each group as one, each annotation
    /// on its own as one; in drawing order of their first member.
    pub fn units(&self, ids: &[ObjectId]) -> Vec<Vec<ObjectId>> {
        let mut units: Vec<(Option<u64>, Vec<ObjectId>)> = Vec::new();
        for o in self.objects.iter().filter(|o| ids.contains(&o.id)) {
            match o.group.and_then(|g| units.iter_mut().find(|(k, _)| *k == Some(g))) {
                Some((_, members)) => members.push(o.id),
                None => units.push((o.group, vec![o.id])),
            }
        }
        units.into_iter().map(|(_, m)| m).collect()
    }

    /// Moves these annotations by `d`.
    pub fn move_objects(&mut self, ids: &[ObjectId], d: Pt) {
        for o in self.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            o.shape.translate(d);
        }
    }

    /// Turns these annotations by `degrees` round `pivot`: boxes go round it and turn; lines,
    /// arrows and strokes have their points turned; a blur or a spotlight goes round it square.
    pub fn rotate_objects(&mut self, ids: &[ObjectId], degrees: f32, pivot: Pt) {
        let deg = crate::object::clamp_rotation(degrees);
        if deg == 0.0 {
            return;
        }
        let pivot = pivot.finite();
        for o in self.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            if o.shape.rect().is_some() {
                let c = o.shape.pivot();
                o.shape.translate(crate::object::turn(c, pivot, deg).sub(c));
                if o.shape.turns() {
                    o.rotation = crate::object::clamp_rotation(o.rotation + deg);
                }
            } else {
                o.shape.map_points(&|q| crate::object::turn(q, pivot, deg));
            }
        }
    }

    /// Mirrors these annotations round the middle of them all, across (`horizontal`) or up and
    /// down; a turned box turns the other way.
    pub fn flip_objects(&mut self, ids: &[ObjectId], horizontal: bool) {
        let Some(c) = self.bounds_of(ids).map(Rect::center) else { return };
        for o in self.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            o.shape.mirror(c, horizontal);
            o.rotation = crate::object::clamp_rotation(-o.rotation);
        }
    }

    /// Lines up these annotations (a group as one) on one side or the middle of the box round
    /// them all, or of `within` when given (the picture, for one alone).
    pub fn align(&mut self, ids: &[ObjectId], to: Align, within: Option<Rect>) {
        let Some(frame) = within.or_else(|| self.bounds_of(ids)) else { return };
        for unit in self.units(ids) {
            let Some(b) = self.bounds_of(&unit) else { continue };
            let d = match to {
                Align::Left => Pt::new(frame.x - b.x, 0.0),
                Align::Center => Pt::new(frame.center().x - b.center().x, 0.0),
                Align::Right => Pt::new(frame.right() - b.right(), 0.0),
                Align::Top => Pt::new(0.0, frame.y - b.y),
                Align::Middle => Pt::new(0.0, frame.center().y - b.center().y),
                Align::Bottom => Pt::new(0.0, frame.bottom() - b.bottom()),
            };
            self.move_objects(&unit, d.finite());
        }
    }

    /// Spreads these annotations (a group as one) evenly across (`horizontal`) or down: the
    /// first and the last stay, and the gaps between them all become the same. Nothing moves
    /// with fewer than three.
    pub fn distribute(&mut self, ids: &[ObjectId], horizontal: bool) {
        let mut units: Vec<(Vec<ObjectId>, Rect)> = self.units(ids).into_iter().filter_map(|u| self.bounds_of(&u).map(|b| (u, b))).collect();
        if units.len() < 3 {
            return;
        }
        let start = |r: &Rect| if horizontal { r.x } else { r.y };
        let size = |r: &Rect| if horizontal { r.w } else { r.h };
        units.sort_by(|a, b| start(&a.1).total_cmp(&start(&b.1)));
        let (Some(first), Some(last)) = (units.first(), units.last()) else { return };
        let span = start(&last.1) + size(&last.1) - start(&first.1);
        let total: f32 = units.iter().map(|(_, r)| size(r)).sum();
        let gap = (span - total) / (units.len() - 1) as f32;
        let mut at = start(&first.1);
        for (unit, r) in &units {
            let d = at - start(r);
            self.move_objects(unit, if horizontal { Pt::new(d, 0.0) } else { Pt::new(0.0, d) }.finite());
            at += size(r) + gap;
        }
    }
}

/// Where annotations are lined up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn doc() -> Document {
        Document::new(Raster::filled(100, 80, Color::WHITE).unwrap())
    }

    fn rect(x: f32) -> Shape {
        Shape::Rect { rect: Rect::new(x, 0.0, 10.0, 10.0), radius: 0.0 }
    }

    #[test]
    fn annotations_moved_together_stop_at_the_edge_together() {
        let mut d = doc();
        let a = d.add(rect(0.0), Style::default()).unwrap();
        let b = d.add(rect(80.0), Style::default()).unwrap();
        let moved = d.limit_move(&[a, b], Pt::new(500.0, 0.0));
        assert!(moved.x < 10.0, "the one nearest the edge decides: {moved:?}");
        assert_eq!(d.limit_move(&[], Pt::new(500.0, 0.0)), Pt::new(500.0, 0.0), "nothing to stop");
        assert_eq!(d.limit_move(&[ObjectId(99)], Pt::new(500.0, 0.0)), Pt::new(500.0, 0.0));
    }

    #[test]
    fn the_picture_grows_see_through_to_hold_what_is_moved_off_it_and_shrinks_back() {
        let mut d = doc();
        let a = d.add(rect(10.0), Style::default()).unwrap();
        // Off the left and the top by 30 and 20: it may go (within reach), and the picture grows.
        let moved = d.limit_move_near(&[a], Pt::new(-40.0, -20.0), MAX_GROW);
        assert_eq!(moved, Pt::new(-40.0, -20.0));
        d.get_mut(a).unwrap().shape.translate(moved);
        let shift = d.grow_to(d.bounds_of(&[a]).unwrap()).unwrap();
        assert!(shift.x >= 30.0 && shift.y >= 20.0, "{shift:?}");
        assert_eq!((d.width(), d.height()), (100 + shift.x as u32, 80 + shift.y as u32));
        // New pixels are see-through; the old picture is where it moved to.
        assert_eq!(d.base.pixel(0, 0).map(|c| c.a), Some(0));
        assert_eq!(d.base.pixel(shift.x as u32 + 5, shift.y as u32 + 5), Some(Color::WHITE));
        let b = d.bounds_of(&[a]).unwrap();
        assert!(d.bounds().encloses(b), "{b:?}");
        // Moved back inside: the growth is not needed, and goes.
        d.get_mut(a).unwrap().shape.translate(Pt::new(60.0, 40.0));
        let keep = Rect::new(shift.x, shift.y, 100.0, 80.0);
        let back = d.shrink_to(keep).unwrap();
        assert_eq!((back, d.width(), d.height()), (Pt::new(-shift.x, -shift.y), 100, 80));
        assert_eq!(d.shrink_to(d.bounds()), None, "nothing more to cut");
        // Nothing to grow for; junk; and a move past its reach is cut down to it.
        assert_eq!(d.grow_to(Rect::new(5.0, 5.0, 10.0, 10.0)), None);
        assert_eq!(d.grow_to(Rect::new(f32::NAN, 0.0, f32::INFINITY, 1.0)), None);
        assert!(d.grow_to(Rect::new(-1.0e30, 0.0, 1.0e30, 10.0)).is_some_and(|s| s.x <= MAX_GROW * 2.0));
        let far = d.limit_move_near(&[a], Pt::new(1.0e9, 0.0), 50.0);
        assert!(far.x > 0.0 && far.x < 1.0e5, "{far:?}");
    }

    #[test]
    fn bringing_inside_moves_all_of_it_and_no_more() {
        let mut d = doc();
        let a = d.add(rect(95.0), Style::default()).unwrap();
        d.bring_inside(a);
        let o = d.get(a).unwrap();
        assert!(d.bounds().encloses(o.shape.bounds(&o.style)), "{:?}", o.shape);
        let before = o.shape.clone();
        d.bring_inside(a);
        assert_eq!(d.get(a).unwrap().shape, before, "already inside");
        d.bring_inside(ObjectId(99));
    }

    #[test]
    fn ids_are_unique_and_inserts_get_fresh_ones() {
        let mut d = doc();
        let a = d.add(rect(0.0), Style::default()).unwrap();
        let b = d.add(rect(20.0), Style::default()).unwrap();
        assert_ne!(a, b);
        let clash = Object::new(a, rect(40.0), Style::default());
        let c = d.insert(clash).unwrap();
        assert!(c != a && c != b);
        let far = Object::new(ObjectId(500), rect(40.0), Style::default());
        assert_eq!(d.insert(far).unwrap(), ObjectId(500));
        assert_eq!(d.add(rect(1.0), Style::default()).unwrap(), ObjectId(501));
        assert_eq!(d.remove(&[a, ObjectId(999)]), 1);
        assert!(d.get(a).is_none());
    }

    #[test]
    fn picking_takes_the_topmost() {
        let mut d = doc();
        let filled = Style { fill: Color::RED, ..Style::default() };
        let a = d.add(rect(0.0), filled).unwrap();
        let b = d.add(rect(5.0), filled).unwrap();
        assert_eq!(d.object_at(Pt::new(7.0, 5.0), 1.0), Some(b));
        assert_eq!(d.object_at(Pt::new(0.5, 5.0), 1.0), Some(a));
        assert_eq!(d.object_at(Pt::new(90.0, 70.0), 1.0), None);
        assert_eq!(d.objects_in(Rect::new(-10.0, -10.0, 25.0, 30.0)), vec![a]);
    }

    #[test]
    fn reordering() {
        let mut d = doc();
        let ids: Vec<_> = (0..4).map(|i| d.add(rect(i as f32), Style::default()).unwrap()).collect();
        let order = |d: &Document| d.objects.iter().map(|o| o.id.0).collect::<Vec<_>>();
        d.reorder(&[ids[0]], Order::Front);
        assert_eq!(order(&d), vec![2, 3, 4, 1]);
        d.reorder(&[ids[0]], Order::Back);
        assert_eq!(order(&d), vec![1, 2, 3, 4]);
        d.reorder(&[ids[1]], Order::Forward);
        assert_eq!(order(&d), vec![1, 3, 2, 4]);
        d.reorder(&[ids[3]], Order::Backward);
        assert_eq!(order(&d), vec![1, 3, 4, 2]);
        d.reorder(&[ids[0]], Order::Backward);
        assert_eq!(order(&d), vec![1, 3, 4, 2], "already at the back");
    }

    #[test]
    fn steps_count_up_and_images_are_pruned() {
        let mut d = doc();
        assert_eq!(d.next_step_number(StepLabels::Numbers), 1);
        d.add(Shape::step(Pt::ZERO, 4), Style::default()).unwrap();
        assert_eq!(d.next_step_number(StepLabels::Numbers), 5);
        // Letters count on their own.
        assert_eq!(d.next_step_number(StepLabels::Lower), 1);
        d.add(Shape::Step { center: Pt::ZERO, number: 2, labels: StepLabels::Lower, badge: Default::default() }, Style::default()).unwrap();
        assert_eq!((d.next_step_number(StepLabels::Lower), d.next_step_number(StepLabels::Numbers)), (3, 5));
        assert!(d.add(Shape::Image { rect: Rect::default(), image: 9 }, Style::default()).is_err());
        let key = d.add_image(Raster::filled(2, 2, Color::BLACK).unwrap()).unwrap();
        let id = d.add(Shape::Image { rect: Rect::new(0.0, 0.0, 2.0, 2.0), image: key }, Style::default()).unwrap();
        d.remove(&[id]);
        assert!(d.images.is_empty());
    }

    #[test]
    fn a_turned_box_is_hit_and_bounded_where_it_shows() {
        let mut d = doc();
        let filled = Style { fill: Color::RED, ..Style::default() };
        let a = d.add(Shape::Rect { rect: Rect::new(30.0, 35.0, 40.0, 10.0), radius: 0.0 }, filled).unwrap();
        // Unturned: wide and low.
        assert_eq!(d.object_at(Pt::new(50.0, 25.0), 0.0), None);
        d.rotate_objects(&[a], 90.0, Pt::new(50.0, 40.0));
        let o = d.get(a).unwrap();
        assert_eq!(o.rotation, 90.0);
        // Turned a quarter: tall and narrow, round the same middle.
        assert_eq!(d.object_at(Pt::new(50.0, 25.0), 0.0), Some(a));
        assert_eq!(d.object_at(Pt::new(35.0, 40.0), 0.0), None);
        let b = o.bounds();
        assert!((b.w - 14.0).abs() < 0.5 && (b.h - 44.0).abs() < 0.5, "{b:?}");
        // Lines turn by their points and keep no turn.
        let l = d.add(Shape::Line { from: Pt::new(10.0, 10.0), to: Pt::new(30.0, 10.0), bend: 0.0 }, Style::default()).unwrap();
        d.rotate_objects(&[l], 90.0, Pt::new(20.0, 10.0));
        let o = d.get(l).unwrap();
        assert_eq!((o.rotation, o.shape.extent()), (0.0, Rect::new(20.0, 0.0, 0.0, 20.0)));
        // A blur goes round square.
        let blur = d.add(Shape::Blur { rect: Rect::new(0.0, 0.0, 10.0, 10.0) }, Style::default()).unwrap();
        d.rotate_objects(&[blur], 180.0, Pt::new(20.0, 20.0));
        assert_eq!((d.get(blur).unwrap().rotation, d.get(blur).unwrap().shape.rect()), (0.0, Some(Rect::new(30.0, 30.0, 10.0, 10.0))));
        // A flip of the picture turns it the other way.
        d.map_objects(&|p| Pt::new(100.0 - p.x, p.y), 1.0);
        d.get_mut(a).unwrap().rotation = 30.0;
        d.map_objects(&|p| Pt::new(100.0 - p.x, p.y), 1.0);
        assert_eq!(d.get(a).unwrap().rotation, -30.0);
        d.map_objects(&|p| Pt::new(80.0 - p.y, p.x), 1.0);
        assert_eq!(d.get(a).unwrap().rotation, -30.0, "a quarter turn of the picture isn't a mirror");
    }

    #[test]
    fn a_turned_box_is_resized_along_its_own_sides() {
        let mut o = Object::new(ObjectId(1), Shape::Rect { rect: Rect::new(0.0, 0.0, 40.0, 20.0), radius: 0.0 }, Style::default());
        o.rotation = 90.0;
        // Its top left corner, on the picture, is at the top right of what shows.
        let corners = o.handles();
        let (_, tl) = corners.iter().find(|(h, _)| *h == crate::Handle::TopLeft).copied().unwrap();
        let (_, br) = corners.iter().find(|(h, _)| *h == crate::Handle::BottomRight).copied().unwrap();
        assert!((tl.x - 30.0).abs() < 0.01 && (tl.y + 10.0).abs() < 0.01, "{tl:?}");
        // Pulled outwards (up the picture is along its width): it grows, the far corner stays.
        o.drag_handle(crate::Handle::TopLeft, Pt::new(tl.x, tl.y - 10.0), false);
        let r = o.shape.rect().unwrap();
        assert!((r.w - 50.0).abs() < 0.01 && (r.h - 20.0).abs() < 0.01, "{r:?}");
        let (_, br_now) = o.handles().into_iter().find(|(h, _)| *h == crate::Handle::BottomRight).unwrap();
        assert!(br_now.distance(br) < 0.01, "{br:?} → {br_now:?}");
        assert_eq!(crate::object::clamp_rotation(270.0), -90.0);
        assert_eq!(crate::object::clamp_rotation(f32::NAN), 0.0);
    }

    #[test]
    fn groups_align_distribute_and_flip_as_units() {
        let mut d = doc();
        let a = d.add(rect(0.0), Style::default()).unwrap();
        let b = d.add(rect(30.0), Style::default()).unwrap();
        let c = d.add(Shape::Rect { rect: Rect::new(70.0, 50.0, 20.0, 20.0), radius: 0.0 }, Style::default()).unwrap();
        let w = Style::default().width / 2.0;
        // a and b grouped: they move as one.
        let g = d.next_group();
        for id in [a, b] {
            d.get_mut(id).unwrap().group = Some(g);
        }
        assert_eq!(d.with_groups(&[b]), vec![a, b]);
        assert_eq!(d.units(&[a, b, c]), vec![vec![a, b], vec![c]]);
        d.align(&[a, b, c], Align::Bottom, None);
        let bottom = |d: &Document, id| d.get(id).unwrap().shape.rect().unwrap().bottom();
        assert_eq!((bottom(&d, a), bottom(&d, b), bottom(&d, c)), (70.0, 70.0, 70.0));
        d.align(&[a, b, c], Align::Left, None);
        let left = |d: &Document, id| d.get(id).unwrap().shape.rect().unwrap().x;
        assert_eq!((left(&d, a), left(&d, b), left(&d, c)), (0.0, 30.0, 0.0), "the group keeps its shape");
        // Three apart, spread evenly.
        let x = d.add(rect(0.0), Style::default()).unwrap();
        let y = d.add(rect(10.0), Style::default()).unwrap();
        let z = d.add(rect(80.0), Style::default()).unwrap();
        d.distribute(&[x, y, z], true);
        assert_eq!(left(&d, y), 40.0);
        // Flipped across round their middle.
        d.flip_objects(&[x, z], true);
        assert_eq!((left(&d, x), left(&d, z)), (80.0, 0.0));
        let _ = w;
        // A stamp of an arrow points the other way.
        let s = d.add(Shape::Stamp { rect: Rect::new(0.0, 0.0, 10.0, 10.0), stamp: crate::StampKind::ArrowLeft }, Style::default()).unwrap();
        d.flip_objects(&[s], true);
        assert!(matches!(d.get(s).unwrap().shape, Shape::Stamp { stamp: crate::StampKind::ArrowRight, .. }));
    }
}
