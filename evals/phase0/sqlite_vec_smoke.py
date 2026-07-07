#!/usr/bin/env python3
"""Phase 0 acceptance check: sqlite-vec vec0 KNN correctness on synthetic vectors.

This mirrors the Rust `chunk_embeddings` design (crate::vector) so we can validate
the vec0 schema + KNN query independently of the (heavy) Rust build. Run:

    python3 -m venv venv && ./venv/bin/pip install sqlite-vec
    ./venv/bin/python evals/phase0/sqlite_vec_smoke.py

Exit code 0 = pass.
"""
import sqlite3
import struct
import sys

import sqlite_vec


def serialize_f32(vec):
    return struct.pack(f"{len(vec)}f", *vec)


def main():
    db = sqlite3.connect(":memory:")
    db.enable_load_extension(True)
    sqlite_vec.load(db)
    db.enable_load_extension(False)

    (vec_version,) = db.execute("SELECT vec_version()").fetchone()
    print(f"sqlite-vec version: {vec_version}")

    # Same shape as crate::vector::ensure_chunk_embeddings_table (dim shrunk for test).
    dim = 4
    db.execute(
        f"CREATE VIRTUAL TABLE chunk_embeddings USING vec0("
        f"chunk_id INTEGER PRIMARY KEY, embedding FLOAT[{dim}])"
    )

    # Synthetic, L2-normalized-ish vectors along distinct axes.
    rows = {
        1: [1.0, 0.0, 0.0, 0.0],
        2: [0.0, 1.0, 0.0, 0.0],
        3: [0.0, 0.0, 1.0, 0.0],
        4: [0.9, 0.1, 0.0, 0.0],  # close to chunk 1
    }
    for cid, v in rows.items():
        db.execute(
            "INSERT INTO chunk_embeddings(chunk_id, embedding) VALUES (?, ?)",
            (cid, serialize_f32(v)),
        )

    query = [1.0, 0.0, 0.0, 0.0]
    results = db.execute(
        "SELECT chunk_id, distance FROM chunk_embeddings "
        "WHERE embedding MATCH ? AND k = 3 ORDER BY distance",
        (serialize_f32(query),),
    ).fetchall()

    print("KNN(query=axis-0, k=3):", results)
    got = [cid for cid, _ in results]
    expected_top2 = [1, 4]  # exact match, then the near-duplicate
    assert got[:2] == expected_top2, f"expected top-2 {expected_top2}, got {got[:2]}"
    assert 2 not in got or got.index(2) > 1, "orthogonal vector ranked too high"

    print("PASS: vec0 KNN returns correct nearest neighbors")
    return 0


if __name__ == "__main__":
    sys.exit(main())
