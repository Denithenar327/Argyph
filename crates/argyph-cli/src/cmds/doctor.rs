use std::process::ExitCode;

pub fn run() -> ExitCode {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let rustc = env!("RUSTC_VERSION");
    let version = env!("CARGO_PKG_VERSION");

    println!("platform: {os}-{arch}");
    println!("rustc: {rustc}");
    println!("argyph: {version}");
    println!("OK");

    ExitCode::SUCCESS
}
