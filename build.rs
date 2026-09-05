fn main() {
    println!("cargo:rerun-if-changed=assets/brand/windows.rc");
    println!("cargo:rerun-if-changed=assets/brand/app-icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/brand/windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Windows application resources should compile");
    }
}
