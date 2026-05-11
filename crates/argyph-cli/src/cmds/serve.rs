use std::process::ExitCode;

use camino::Utf8PathBuf;

pub fn run() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("argyph serve: cannot determine current directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let root = match Utf8PathBuf::from_path_buf(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "argyph serve: current directory is not valid UTF-8: {}",
                e.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let config = match argyph_core::Config::load(&root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("argyph serve: failed to load config: {e}");
            return ExitCode::FAILURE;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("argyph serve: failed to create async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        let supervisor = match argyph_core::Supervisor::boot(root.clone(), config).await {
            Ok(s) => std::sync::Arc::new(s),
            Err(e) => {
                eprintln!("argyph serve: boot failed: {e}");
                return;
            }
        };

        if let Err(e) = argyph_mcp::serve(supervisor, root).await {
            eprintln!("argyph serve: error: {e}");
        }
    });

    ExitCode::SUCCESS
}
