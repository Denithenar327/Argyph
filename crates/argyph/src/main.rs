use clap::Parser;
use std::process::ExitCode;

fn run() -> ExitCode {
    let cli = argyph_cli::Cli::parse();
    argyph_cli::dispatch(cli.command)
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_env_var("ARGYPH_LOG")
                .with_default_directive(tracing::level_filters::LevelFilter::WARN.into())
                .from_env_lossy(),
        )
        .init();

    let code = run();

    if code != ExitCode::SUCCESS {
        eprintln!("Hint: set ARGYPH_LOG=debug for more detail.");
    }

    code
}
