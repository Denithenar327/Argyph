use std::collections::HashMap;

/// A single result from a hybrid search query.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub chunk_id: String,
    pub chunk_text: String,
    pub file: String,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub score: f32,
    pub source: HitSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSource {
    Bm25,
    Vector,
    Hybrid,
}

/// Optional filters for search queries.
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub language: Option<String>,
    pub paths_glob: Option<String>,
    pub exclude_glob: Option<String>,
    pub file_ids: Option<Vec<i64>>,
}

/// Result of a hybrid search, including diagnostic counts.
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    pub hits: Vec<SearchHit>,
    pub total_embedded: usize,
    pub total_chunks: usize,
}

/// A vector to store for a chunk.
#[derive(Debug, Clone)]
pub struct VectorEntry {
    pub chunk_id: String,
    pub vector: Vec<f32>,
    pub model: String,
    pub dimension: usize,
}

/// Reciprocal rank fusion constant.
pub(crate) const RRF_K: f64 = 60.0;

/// Fuse two ranked lists of `(chunk_id, score)` via reciprocal rank fusion.
///
/// Each chunk's RRF score is `1 / (RRF_K + rank)`. A chunk appearing in both
/// lists gets summed scores.
pub fn reciprocal_rank_fusion(
    bm25_hits: &[(String, f32)],
    vector_hits: &[(String, f32)],
    k: usize,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f64> =
        HashMap::with_capacity(bm25_hits.len() + vector_hits.len());

    for (rank, (chunk_id, _)) in bm25_hits.iter().enumerate() {
        let rrf = 1.0 / (RRF_K + (rank + 1) as f64);
        *scores.entry(chunk_id.clone()).or_insert(0.0) += rrf;
    }

    for (rank, (chunk_id, _)) in vector_hits.iter().enumerate() {
        let rrf = 1.0 / (RRF_K + (rank + 1) as f64);
        *scores.entry(chunk_id.clone()).or_insert(0.0) += rrf;
    }

    let mut results: Vec<_> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(k);

    results
        .into_iter()
        .map(|(id, score)| (id, score as f32))
        .collect()
}
