use std::{env, fs};

#[path = "src/dotenv_file.rs"]
mod dotenv_file;

const EMBEDDED_ENV_KEYS: [&str; 2] = ["AZULC_CURSEFORGE_API_KEY", "AZULC_MICROSOFT_CLIENT_ID"];

fn main() {
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-changed=assets/brand/windows.rc");
    println!("cargo:rerun-if-changed=assets/brand/app-icon.ico");

    embed_environment();

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/brand/windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Windows application resources should compile");
    }
}

fn embed_environment() {
    let dotenv = fs::read_to_string(".env")
        .expect(".env must exist when building AZULC's embedded configuration");

    for key in EMBEDDED_ENV_KEYS {
        let value = dotenv_file::literal_value(&dotenv, key)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| panic!("{key} must have a non-empty value in .env"));
        assert!(
            !value.contains('\r') && !value.contains('\n'),
            "{key} cannot contain a newline"
        );
        println!("cargo:rustc-env={key}={value}");
    }
}
