// Windows: the .exe gets the app icon. Adapted from SolidCraft (tangency-labs/solidcraft, MIT OR Apache-2.0).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon.ico");
    let rc = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("icon.rc");
    std::fs::write(&rc, format!("1 ICON \"{}\"\n", icon.display().to_string().replace('\\', "/"))).expect("write icon.rc");
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().expect("embed the icon");
}
