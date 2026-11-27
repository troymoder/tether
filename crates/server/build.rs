fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tetherd builds for Linux and macOS only (main.rs), so other targets need
    // no client.
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if !matches!(target.as_str(), "linux" | "macos") {
        return Ok(());
    }

    println!("cargo:rerun-if-env-changed=EMULATOR_PROTOS");
    let folder = std::env::var("EMULATOR_PROTOS")
        .map_err(|_| "EMULATOR_PROTOS is not set: build in the dev shell")?;
    println!("cargo:rerun-if-changed={folder}/emulator_controller.proto");

    let files = protox::compile(["emulator_controller.proto"], [folder])?;
    tonic_prost_build::configure()
        .build_server(false)
        .build_transport(false)
        .compile_fds(files)?;

    Ok(())
}
