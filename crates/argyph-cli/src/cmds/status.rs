use std::process::ExitCode;

use camino::Utf8PathBuf;

use crate::output;

pub fn run() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("argyph status: {e}");
            return ExitCode::FAILURE;
        }
    };
    let root = match Utf8PathBuf::from_path_buf(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("argyph status: not valid UTF-8: {}", e.display());
            return ExitCode::FAILURE;
        }
    };

    let config = match argyph_core::Config::load(&root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("argyph status: config error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("argyph status: async runtime error: {e}");
            return ExitCode::FAILURE;
        }
    };

    rt.block_on(async {
        let sup = match argyph_core::Supervisor::boot(root.clone(), config).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("argyph status: {e}");
                return;
            }
        };

        let tier_state = sup.get_tier_state().await;
        let index = sup.index();
        let status = match index.status().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("argyph status: {e}");
                return;
            }
        };

        let files = match index.list_files().await {
            Ok(f) => f,
            Err(e) => {
                eprintln!("argyph status: {e}");
                return;
            }
        };

        output::status_table(&tier_state, &status, &files, sup.watcher_active());
        let _ = sup.shutdown().await;
    });

    ExitCode::SUCCESS
}
