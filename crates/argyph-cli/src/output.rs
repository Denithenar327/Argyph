use argyph_core::{IndexStatus, TierState};
use argyph_fs::FileEntry;

pub fn status_table(
    tier_state: &TierState,
    status: &IndexStatus,
    files: &[FileEntry],
    watcher_active: bool,
) {
    println!("Index Status");
    println!("  tier:       {tier_state}");
    println!("  ready:      {}", tier_state.is_ready());
    println!("  tier number: {}", tier_state.tier_number());
    println!("  files:      {}", status.file_count);
    println!(
        "  watcher:    {}",
        if watcher_active { "active" } else { "inactive" }
    );
    println!("  version:    {}", status.protocol_version);

    if !files.is_empty() {
        let mut lang_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for f in files {
            if let Some(lang) = &f.language {
                *lang_counts.entry(lang.to_string()).or_default() += 1;
            }
        }
        println!("  languages:");
        let mut pairs: Vec<_> = lang_counts.into_iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(&a.1));
        for (lang, count) in &pairs {
            println!("    {lang}: {count}");
        }
    }
}
