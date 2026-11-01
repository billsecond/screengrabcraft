//! A picture being worked on: the captured (or opened) picture, the annotations on it in the
//! order they are drawn, and the pictures placed on it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::{Pt, Rect};
use crate::object::{Kind, Object, ObjectId, Shape, Style};
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
        self.objects.push(Object { id, shape: shape.sanitized(), style: style.clamped() });
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
        object.shape = object.shape.sanitized();
        object.style = object.style.clamped();
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
        self.objects.iter().rev().find(|o| o.shape.hit(&o.style, p, tol)).map(|o| o.id)
    }

    /// Annotations whose bounds lie inside `area` (a rubber-band selection).
    pub fn objects_in(&self, area: Rect) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|o| {
                let b = o.shape.bounds(&o.style);
                area.contains(b.min()) && area.contains(b.max())
            })
            .map(|o| o.id)
            .collect()
    }

    /// `d` cut down so that these annotations, moved by it together, stay on the picture.
    pub fn limit_move(&self, ids: &[ObjectId], d: Pt) -> Pt {
        let area = ids.iter().filter_map(|id| self.get(*id)).map(|o| o.shape.bounds(&o.style)).reduce(Rect::union);
        area.map_or(d, |a| a.limit_move(d, self.bounds()))
    }

    /// Moves an annotation onto the picture, as little as it takes.
    pub fn bring_inside(&mut self, id: ObjectId) {
        let bounds = self.bounds();
        if let Some(o) = self.get_mut(id) {
            o.shape.translate(o.shape.bounds(&o.style).way_into(bounds));
        }
    }

    /// The number the next step badge gets: one more than the highest so far.
    pub fn next_step_number(&self) -> u32 {
        self.objects
            .iter()
            .filter_map(|o| match o.shape {
                Shape::Step { number, .. } => Some(number),
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
        for o in &mut self.objects {
            o.shape.map_points(f);
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
        let clash = Object { id: a, shape: rect(40.0), style: Style::default() };
        let c = d.insert(clash).unwrap();
        assert!(c != a && c != b);
        let far = Object { id: ObjectId(500), shape: rect(40.0), style: Style::default() };
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
        assert_eq!(d.next_step_number(), 1);
        d.add(Shape::Step { center: Pt::ZERO, number: 4 }, Style::default()).unwrap();
        assert_eq!(d.next_step_number(), 5);
        assert!(d.add(Shape::Image { rect: Rect::default(), image: 9 }, Style::default()).is_err());
        let key = d.add_image(Raster::filled(2, 2, Color::BLACK).unwrap()).unwrap();
        let id = d.add(Shape::Image { rect: Rect::new(0.0, 0.0, 2.0, 2.0), image: key }, Style::default()).unwrap();
        d.remove(&[id]);
        assert!(d.images.is_empty());
    }
}
