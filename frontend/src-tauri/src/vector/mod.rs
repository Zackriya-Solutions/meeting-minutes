//! Local vector search backed by the `sqlite-vec` extension (PLAN.md Phase 0/1).
//!
//! Everything here is best-effort and degrades gracefully: if the extension cannot
//! be registered or the `chunk_embeddings` virtual table cannot be created, the app
//! still boots and hybrid search (Phase 1) falls back to FTS-only. Nothing in this
//! module is allowed to panic on the startup path.
//!
//! ADAPTATION NOTE (see migration 20260706000000): `chunks.id` is an INTEGER rowid,
//! so it maps directly onto vec0's INTEGER primary key. Meeting ids are TEXT, but
//! embeddings key off `chunk_id`, so no TEXT/INTEGER mismatch reaches this layer.

use std::sync::Once;

use sqlx::SqlitePool;

/// Embedding dimension for the `chunk_embeddings` vec0 table.
///
/// Default = 384 (`intfloat/multilingual-e5-small`), a strong, small multilingual
/// encoder with good Russian recall. PLAN.md §11 open-decision #1 reconfirms this
/// after the Phase 1 benchmark; because the table is created in code (not a hard
/// migration) and holds no data until Phase 1, changing this constant only requires
/// dropping/recreating the empty table.
pub const EMBEDDING_DIM: usize = 384;

static REGISTER: Once = Once::new();

/// Register the sqlite-vec extension as an SQLite auto-extension so it loads on
/// every connection opened afterwards. MUST be called before the `SqlitePool` is
/// created. Idempotent (guarded by `Once`); safe to call from multiple entry points.
pub fn register() {
    REGISTER.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is the extension entry point exported by the
        // sqlite-vec crate; `sqlite3_auto_extension` expects an
        // `Option<unsafe extern "C" fn()>`. This is the vendor-documented
        // registration pattern for using sqlite-vec with sqlx. It relies on
        // `libsqlite3-sys` linking the same SQLite as sqlx (see Cargo.toml note).
        unsafe {
            libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
        log::info!("sqlite-vec registered as auto-extension");
    });
}

/// Create the `chunk_embeddings` vec0 virtual table if the extension is available.
/// Returns `Ok(true)` when vector search is usable, `Ok(false)` when the extension
/// is unavailable (logged, not fatal).
pub async fn ensure_chunk_embeddings_table(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    // Probe: is the extension actually loaded on this pool's connections?
    match sqlx::query_scalar::<_, String>("SELECT vec_version()")
        .fetch_one(pool)
        .await
    {
        Ok(version) => log::info!("sqlite-vec available (version {version})"),
        Err(e) => {
            log::warn!(
                "sqlite-vec extension not available ({e}); vector search disabled, \
                 hybrid search will fall back to FTS-only"
            );
            return Ok(false);
        }
    }

    let ddl = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_embeddings USING vec0(\
         chunk_id INTEGER PRIMARY KEY, embedding FLOAT[{EMBEDDING_DIM}])"
    );
    if let Err(e) = sqlx::query(&ddl).execute(pool).await {
        log::warn!("failed to create chunk_embeddings vec0 table ({e}); vector search disabled");
        return Ok(false);
    }

    log::info!("chunk_embeddings vec0 table ready (dim={EMBEDDING_DIM})");
    Ok(true)
}

/// Serialize an embedding into the little-endian f32 byte layout vec0 expects.
/// The embedder (Phase 1) is responsible for L2-normalizing before calling this.
pub fn serialize_embedding(vec: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(vec).to_vec()
}

/// Upsert one embedding for a chunk. No-op-safe to call repeatedly (deterministic
/// chunker + `INSERT OR REPLACE` keeps backfill idempotent).
pub async fn upsert_embedding(
    pool: &SqlitePool,
    chunk_id: i64,
    embedding: &[f32],
) -> Result<(), sqlx::Error> {
    let bytes = serialize_embedding(embedding);
    sqlx::query("INSERT OR REPLACE INTO chunk_embeddings(chunk_id, embedding) VALUES (?, ?)")
        .bind(chunk_id)
        .bind(bytes)
        .execute(pool)
        .await?;
    Ok(())
}

/// K-nearest-neighbor search over chunk embeddings. Returns `(chunk_id, distance)`
/// ordered nearest-first. Used by the Phase 1 hybrid engine's vector branch.
pub async fn knn(
    pool: &SqlitePool,
    query_embedding: &[f32],
    k: i64,
) -> Result<Vec<(i64, f64)>, sqlx::Error> {
    let bytes = serialize_embedding(query_embedding);
    let rows: Vec<(i64, f64)> = sqlx::query_as(
        "SELECT chunk_id, distance FROM chunk_embeddings \
         WHERE embedding MATCH ? AND k = ? ORDER BY distance",
    )
    .bind(bytes)
    .bind(k)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Exact KNN over an allowed chunk-id set. Batching avoids SQLite parameter limits;
/// merging each batch's top-k is equivalent to global top-k over the allowed set.
pub async fn knn_filtered(
    pool: &SqlitePool,
    query_embedding: &[f32],
    allowed_chunk_ids: &[i64],
    k: i64,
) -> Result<Vec<(i64, f64)>, sqlx::Error> {
    if allowed_chunk_ids.is_empty() || k <= 0 {
        return Ok(Vec::new());
    }
    let bytes = serialize_embedding(query_embedding);
    let mut candidates = Vec::new();
    for batch in allowed_chunk_ids.chunks(400) {
        let placeholders = vec!["?"; batch.len()].join(",");
        let sql = format!(
            "SELECT chunk_id, vec_distance_cosine(embedding, ?) AS distance \
             FROM chunk_embeddings WHERE chunk_id IN ({placeholders}) \
             ORDER BY distance, chunk_id LIMIT ?"
        );
        let mut query = sqlx::query_as::<_, (i64, f64)>(&sql).bind(bytes.clone());
        for id in batch {
            query = query.bind(*id);
        }
        candidates.extend(query.bind(k).fetch_all(pool).await?);
    }
    candidates.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    candidates.dedup_by_key(|row| row.0);
    candidates.truncate(k as usize);
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Mirrors evals/phase0/sqlite_vec_smoke.py: exact match ranks first, then the
    /// near-duplicate, orthogonal vector last.
    #[tokio::test]
    async fn vec0_knn_returns_correct_neighbors() {
        register();
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory db");

        // Use a small dim table directly (independent of EMBEDDING_DIM) for a
        // readable synthetic test.
        sqlx::query(
            "CREATE VIRTUAL TABLE chunk_embeddings USING vec0(\
             chunk_id INTEGER PRIMARY KEY, embedding FLOAT[4])",
        )
        .execute(&pool)
        .await
        .expect("create vec0 table (is sqlite-vec linked?)");

        let rows: [(i64, [f32; 4]); 4] = [
            (1, [1.0, 0.0, 0.0, 0.0]),
            (2, [0.0, 1.0, 0.0, 0.0]),
            (3, [0.0, 0.0, 1.0, 0.0]),
            (4, [0.9, 0.1, 0.0, 0.0]),
        ];
        for (id, v) in rows {
            upsert_embedding(&pool, id, &v).await.expect("insert");
        }

        let results = knn(&pool, &[1.0, 0.0, 0.0, 0.0], 3).await.expect("knn");
        let ids: Vec<i64> = results.iter().map(|(id, _)| *id).collect();
        assert_eq!(&ids[..2], &[1, 4], "expected exact match then near-duplicate");
        assert!(!ids[..2].contains(&2), "orthogonal vector ranked too high");

        let filtered = knn_filtered(&pool, &[1.0, 0.0, 0.0, 0.0], &[2, 3, 4], 2)
            .await
            .expect("filtered knn");
        assert_eq!(filtered.iter().map(|row| row.0).collect::<Vec<_>>(), vec![4, 2]);
    }
}
