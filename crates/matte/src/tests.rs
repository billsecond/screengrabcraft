use prost::Message;
use screengrabcraft_model::{Color, Raster};
use tract_onnx::pb;

use super::*;

/// A tiny ONNX model of the U²-Net family's shape: 1×3×320×320 in, the mean of the three
/// channels out (1×1×320×320). Bright is "subject".
fn tiny_model() -> Vec<u8> {
    let tensor = |name: &str, dims: &[i64]| pb::ValueInfoProto {
        name: name.to_string(),
        r#type: Some(pb::TypeProto {
            value: Some(pb::type_proto::Value::TensorType(pb::type_proto::Tensor {
                elem_type: pb::tensor_proto::DataType::Float as i32,
                shape: Some(pb::TensorShapeProto {
                    dim: dims
                        .iter()
                        .map(|d| pb::tensor_shape_proto::Dimension {
                            value: Some(pb::tensor_shape_proto::dimension::Value::DimValue(*d)),
                            ..Default::default()
                        })
                        .collect(),
                }),
            })),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mean = pb::NodeProto {
        input: vec!["input".to_string()],
        output: vec!["output".to_string()],
        name: "mean".to_string(),
        op_type: "ReduceMean".to_string(),
        attribute: vec![
            pb::AttributeProto {
                name: "axes".to_string(),
                r#type: pb::attribute_proto::AttributeType::Ints as i32,
                ints: vec![1],
                ..Default::default()
            },
            pb::AttributeProto { name: "keepdims".to_string(), r#type: pb::attribute_proto::AttributeType::Int as i32, i: 1, ..Default::default() },
        ],
        ..Default::default()
    };
    let model = pb::ModelProto {
        ir_version: 7,
        opset_import: vec![pb::OperatorSetIdProto { domain: String::new(), version: 13 }],
        graph: Some(pb::GraphProto {
            node: vec![mean],
            name: "tiny".to_string(),
            input: vec![tensor("input", &[1, 3, 320, 320])],
            output: vec![tensor("output", &[1, 1, 320, 320])],
            ..Default::default()
        }),
        ..Default::default()
    };
    model.encode_to_vec()
}

fn half_and_half(w: u32, h: u32) -> Raster {
    let mut r = Raster::filled(w, h, Color::BLACK).unwrap();
    let half = (w / 2) as usize;
    for row in r.pixels_mut().as_chunks_mut::<4>().0.chunks_mut(w as usize) {
        for px in &mut row[half..] {
            *px = [255, 255, 255, 255];
        }
    }
    r
}

fn remover(bytes: Vec<u8>) -> Remover {
    Remover::new("tiny", Family::U2Net, Box::new(move || Ok(bytes.clone())))
}

#[test]
fn a_model_runs_end_to_end() {
    let m = remover(tiny_model());
    let pic = half_and_half(200, 120);
    let mut seen = Vec::new();
    let matte = m.matte(&pic, &mut |f| {
        seen.push(f);
        true
    });
    let matte = matte.unwrap();
    assert_eq!((matte.width(), matte.height()), (200, 120));
    assert_eq!(matte.get(10, 60), Some(0), "the dark half is background");
    assert_eq!(matte.get(190, 60), Some(255), "the bright half is subject");
    assert!(matte.get(100, 60).is_some_and(|a| a > 0 && a < 255), "a soft edge between");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]) && seen.last() == Some(&1.0), "{seen:?}");
    assert_eq!(m.name(), "tiny");
}

#[test]
fn tiny_and_huge_pictures_work() {
    let m = remover(tiny_model());
    let one = m.matte(&Raster::single(Color::RED), &mut |_| true).unwrap();
    assert_eq!((one.width(), one.height()), (1, 1));
    // A long scrolling capture: averaged down, the matte at full size.
    let long = half_and_half(1500, 30_000);
    let matte = m.matte(&long, &mut |_| true).unwrap();
    assert_eq!((matte.width(), matte.height()), (1500, 30_000));
    assert_eq!(matte.get(5, 29_000), Some(0));
    assert_eq!(matte.get(1495, 10), Some(255));
}

#[test]
fn cancelling_stops_it() {
    let m = remover(tiny_model());
    let pic = half_and_half(50, 50);
    assert_eq!(m.matte(&pic, &mut |_| false), Err(MatteError::Cancelled));
    let mut calls = 0;
    let r = m.matte(&pic, &mut |_| {
        calls += 1;
        calls < 2
    });
    assert_eq!(r, Err(MatteError::Cancelled));
    assert!(m.matte(&pic, &mut |_| true).is_ok(), "and it still works after");
}

#[test]
fn a_bad_model_file_is_an_error_not_a_crash() {
    let pic = half_and_half(20, 20);
    let good = tiny_model();
    let cut = good[..good.len() / 2].to_vec();
    let mut flipped = good.clone();
    for b in flipped.iter_mut().skip(10).step_by(7) {
        *b ^= 0x5A;
    }
    for bytes in [Vec::new(), b"not a model at all".to_vec(), cut, flipped, vec![0xFF; 4096]] {
        let m = remover(bytes);
        assert!(matches!(m.matte(&pic, &mut |_| true), Err(MatteError::Model(_))));
    }
    let unreadable = Remover::new("gone", Family::IsNet, Box::new(|| Err("no such file".to_string())));
    assert_eq!(unreadable.matte(&pic, &mut |_| true), Err(MatteError::Model("no such file".to_string())));
}

#[test]
fn a_model_of_another_shape_is_an_error() {
    // The tiny model wants 320 × 320; told it's IS-Net (1024 × 1024), it can't be readied.
    let m = Remover::new("wrong", Family::IsNet, Box::new(|| Ok(tiny_model())));
    assert!(matches!(m.matte(&half_and_half(20, 20), &mut |_| true), Err(MatteError::Model(_))));
}

#[test]
fn families_are_known_by_name() {
    assert_eq!(Family::from_name("isnet-general-use.onnx"), Some(Family::IsNet));
    assert_eq!(Family::from_name("U2NETP.onnx"), Some(Family::U2Net));
    assert_eq!(Family::from_name("BiRefNet-general-lite.onnx"), Some(Family::BiRefNet));
    assert_eq!(Family::from_name("model.onnx"), None);
}

/// The real model, when its file is there (`assets/models`, or `SCREENGRABCRAFT_MATTE_MODEL`):
/// `cargo test -p screengrabcraft-matte -- --ignored`.
#[test]
#[ignore = "needs the model file"]
fn the_real_model_finds_a_subject() {
    let path = std::env::var("SCREENGRABCRAFT_MATTE_MODEL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/isnet-general-use-int8.onnx"));
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("no model at {}", path.display());
        return;
    };
    let family = Family::from_name(&path.to_string_lossy()).unwrap_or(Family::IsNet);
    let m = Remover::new("real", family, Box::new(move || Ok(bytes.clone())));
    // A made-up picture: a red ball with a highlight, on a plain light grey wall.
    let mut pix = screengrabcraft_render::canvas::blank(640, 480).unwrap();
    pix.fill(tiny_skia::Color::from_rgba8(225, 225, 220, 255));
    let mut paint = tiny_skia::Paint { anti_alias: true, ..tiny_skia::Paint::default() };
    paint.set_color_rgba8(200, 30, 40, 255);
    let ball = tiny_skia::PathBuilder::from_circle(320.0, 260.0, 140.0).unwrap();
    pix.fill_path(&ball, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
    paint.set_color_rgba8(255, 200, 200, 255);
    let shine = tiny_skia::PathBuilder::from_circle(270.0, 200.0, 30.0).unwrap();
    pix.fill_path(&shine, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
    let pic = screengrabcraft_render::canvas::from_pixmap(&pix).unwrap();
    let started = std::time::Instant::now();
    let matte = m.matte(&pic, &mut |_| true).unwrap();
    eprintln!("{} in {:.1} s", path.display(), started.elapsed().as_secs_f32());
    assert!(matte.get(320, 260).is_some_and(|a| a > 200), "the ball is kept");
    assert!(matte.get(10, 10).is_some_and(|a| a < 40), "the wall goes");
    assert!(matte.get(630, 470).is_some_and(|a| a < 40));
}
