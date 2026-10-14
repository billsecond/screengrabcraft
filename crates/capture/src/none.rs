//! Systems without a way to capture the screen.

use screengrabcraft_model::Raster;

use crate::{CaptureError, Frozen, Monitor, Permission, WindowInfo};

pub fn name() -> String {
    "none".to_string()
}

pub fn monitors() -> Result<Vec<Monitor>, CaptureError> {
    Err(CaptureError::Unsupported)
}

pub fn windows() -> Result<Vec<WindowInfo>, CaptureError> {
    Ok(Vec::new())
}

pub fn freeze() -> Result<Frozen, CaptureError> {
    Err(CaptureError::Unsupported)
}

pub fn grab_monitor(_id: u32) -> Result<(Monitor, Raster), CaptureError> {
    Err(CaptureError::Unsupported)
}

pub fn pointer() -> Option<(i32, i32)> {
    None
}

pub fn permission() -> Permission {
    Permission::Denied
}

pub fn request_permission() -> bool {
    false
}
