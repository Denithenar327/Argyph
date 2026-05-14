use std::path::Path;
use std::sync::Arc;

pub mod path;
pub mod resolve;
pub mod smart;
pub mod strategy;
pub mod types;

pub use types::*;

pub async fn locate(
    store: Arc<dyn argyph_store::Store>,
    embedder: Arc<dyn argyph_embed::Embedder>,
    root: &Path,
    req: Request,
) -> anyhow::Result<Response> {
    if req.query.is_none() && req.path.is_none() {
        anyhow::bail!("INVALID_ARGUMENT: query or path required");
    }
    if req.file.is_some() && req.files.is_some() {
        anyhow::bail!("INVALID_ARGUMENT: file and files are mutually exclusive");
    }
    if req.path.is_some() && req.query.is_some() && req.file.is_none() {
        anyhow::bail!("INVALID_ARGUMENT: scoped query requires `file`");
    }

    let max_results = req.max_results.clamp(1, 10) as usize;

    let single_file: Option<i64> = if let Some(ref f) = req.file {
        store.get_file_id(&camino::Utf8PathBuf::from(f)).await?
    } else {
        None
    };

    let file_ids: Option<Vec<i64>> = if req.file.is_some() {
        single_file.map(|id| vec![id])
    } else {
        None
    };

    let has_tier2 = true;
    let coverage = types::IndexCoverage {
        tier_1_5: "ready".into(),
        tier_2: if has_tier2 {
            "ready".into()
        } else {
            "building".into()
        },
    };

    let p = strategy::plan(req.query.as_deref(), req.path.as_deref(), has_tier2);
    let strategy_used = strategy::strategy_of(&p);

    let spans = match p {
        strategy::Plan::StructuralPath { locator } => {
            resolve::resolve_structural_path(
                store,
                root,
                &locator,
                single_file,
                req.max_bytes_per_span,
            )
            .await?
        }
        strategy::Plan::StructuralSearch { query } => {
            resolve::resolve_structural_search(
                Arc::clone(&store),
                root,
                &query,
                file_ids.as_deref(),
                max_results,
                req.max_bytes_per_span,
            )
            .await?
        }
        strategy::Plan::Semantic { query } | strategy::Plan::Hybrid { query } => {
            resolve::resolve_hybrid(
                Arc::clone(&store),
                root,
                embedder,
                &query,
                file_ids.as_deref(),
                max_results,
                req.max_bytes_per_span,
            )
            .await?
        }
        strategy::Plan::ScopedSemantic { locator, query } => {
            resolve::resolve_scoped_semantic(
                Arc::clone(&store),
                root,
                embedder,
                &locator,
                &query,
                single_file,
                max_results,
                req.max_bytes_per_span,
            )
            .await?
        }
    };

    Ok(Response {
        spans,
        strategy_used,
        index_coverage: coverage,
    })
}
