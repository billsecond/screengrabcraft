//! The commit and the date of a release build, baked into `--version` and About.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SCREENGRABCRAFT_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=SCREENGRABCRAFT_BUILD_DATE");
    println!("cargo:rerun-if-env-changed=SCREENGRABCRAFT_VERSION");
}
