use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../assets/windows/kea.rc");
    println!("cargo:rerun-if-changed=../../assets/windows/kea.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let assets =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
            .join("../../assets/windows");
    embed_resource::compile_for(
        assets.join("kea.rc"),
        ["kea"],
        embed_resource::ParamsIncludeDirs([assets.as_os_str()]),
    )
    .manifest_required()
    .expect("Kea Windows application icon must be embedded");
}
