// System-level timing harness: boots a Supervisor against a real
// repository and reports cold/warm tier-readiness latencies. The
// numbers feed `docs/benchmarks.md`'s system-level table and the
// README. Run via `cargo run --release -p argyph-benches --bin
// system_bench -- /path/to/repo`.

use std::time::Instant;

use argyph_core::config::Config;
use argyph_core::supervisor::Supervisor;
use argyph_core::tiers::TierState;
use camino::Utf8PathBuf;

#[allow(clippy::expect_used)]
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let root_arg = std::env::args()
        .nth(1)
        .expect("usage: system_bench <repo-path>");
    let root = Utf8PathBuf::from(root_arg);
    let cache = root.join(".argyph");
    let warm = cache.exists();

    let label = if warm { "warm" } else { "cold" };
    println!("== system_bench ({label}) on {root}");

    let start = Instant::now();
    let sup = Supervisor::boot(root.clone(), Config::default())
        .await
        .expect("supervisor boot");
    let tier0_ready = start.elapsed();
    let initial = sup.get_tier_state().await;

    println!("tier0_or_better_ready: {tier0_ready:?}  (state={initial})");

    // Poll for tier readiness. Cap is overridable via ARGYPH_BENCH_CAP_SECS
    // so large repos can run to completion.
    let cap_secs: u64 = std::env::var("ARGYPH_BENCH_CAP_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let t1_deadline = Instant::now() + std::time::Duration::from_secs(cap_secs);
    let mut tier1_at = None;
    let mut tier2_at = None;
    let mut last_state = String::new();
    while Instant::now() < t1_deadline {
        let s = sup.get_tier_state().await;
        let s_str = s.to_string();
        if s_str != last_state {
            eprintln!(
                "state transition: {last_state:?} -> {s_str:?} @ {:?}",
                start.elapsed()
            );
            last_state = s_str;
        }
        match s {
            TierState::Tier1 { .. }
            | TierState::Tier1_5 { .. }
            | TierState::Tier2 { .. }
            | TierState::Ready => {
                if tier1_at.is_none() {
                    tier1_at = Some(start.elapsed());
                }
                if matches!(s, TierState::Tier2 { .. } | TierState::Ready) && tier2_at.is_none() {
                    tier2_at = Some(start.elapsed());
                }
            }
            _ => {}
        }
        if tier1_at.is_some() && tier2_at.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    if let Some(t) = tier1_at {
        println!("tier1_ready: {t:?}");
    } else {
        println!("tier1_ready: TIMEOUT (>120s)");
    }
    if let Some(t) = tier2_at {
        println!("tier2_ready: {t:?}");
    } else {
        println!("tier2_ready: not reached during window");
    }
}
