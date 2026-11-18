fn main() {
    // Official builds from the release repository set this.
    println!("cargo:rerun-if-env-changed=PAWCHIVE_BUILD_CHANNEL");
    tauri_build::build()
}
