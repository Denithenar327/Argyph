use std::process::ExitCode;

pub fn run() -> ExitCode {
    println!("argyph doctor");
    println!("  version:  {}", env!("CARGO_PKG_VERSION"));
    println!(
        "  os:       {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    let rustc = option_env!("RUSTC_VERSION").unwrap_or("unknown");
    println!("  rustc:    {rustc}");

    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            println!("  cwd:      (error: {e})");
            return ExitCode::SUCCESS;
        }
    };
    println!("  cwd:      {}", cwd.display());

    let git = std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "not found".to_string());
    println!("  git:      {git}");

    let rg = std::process::Command::new("rg")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "not found (text search will use built-in regex)".to_string());
    println!("  rg:       {rg}");

    if let Ok(root) = camino::Utf8PathBuf::from_path_buf(cwd.clone()) {
        let config = match argyph_core::Config::load(&root) {
            Ok(c) => c,
            Err(e) => {
                println!("  config:   (error: {e})");
                return ExitCode::SUCCESS;
            }
        };
        println!("  config languages: {:?}", config.index.languages);
        let index_path = root.join(".argyph").join("meta.sqlite");
        println!(
            "  index:    {} (exists: {})",
            index_path.as_str(),
            index_path.exists()
        );
    }

    ExitCode::SUCCESS
}
