use std::process::Command;

fn main() {
    let rustc = option_env!("RUSTC").unwrap_or("rustc");
    if let Ok(output) = Command::new(rustc).arg("--version").output() {
        let version = String::from_utf8_lossy(&output.stdout);
        println!("cargo:rustc-env=RUSTC_VERSION={}", version.trim());
    } else {
        println!("cargo:rustc-env=RUSTC_VERSION=unknown");
    }
}
