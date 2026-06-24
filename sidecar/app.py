"""
Meetily local ML sidecar — 127.0.0.1 only.

Endpoints (all local, no outbound network except localhost Ollama):
  GET  /health
  POST /translate  -> Thai→English with technical terms pinned (translategemma via Ollama)
  POST /embed      -> bge-m3 embeddings via Ollama
  POST /segment    -> Thai word segmentation (PyThaiNLP newmm)
  POST /index      -> store meeting chunks into sqlite-vec + FTS5
  POST /search     -> hybrid (dense + sparse) search

Heavy ML lives here so the Tauri/Rust core stays thin. Models run via the
local Ollama server; nothing leaves the machine.
"""
from __future__ import annotations

import json
import os
import sqlite3
from typing import List, Optional

import httpx
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

OLLAMA_URL = os.environ.get("OLLAMA_URL", "http://127.0.0.1:11434").rstrip("/")
EMBED_DIM = int(os.environ.get("MEET_EMBED_DIM", "1024"))  # bge-m3 = 1024

# ── Optional deps degrade gracefully ──────────────────────────────────────────
try:
    from pythainlp.tokenize import word_tokenize as _thai_tokenize  # type: ignore

    def thai_segment(text: str) -> List[str]:
        return [t for t in _thai_tokenize(text, engine="newmm", keep_whitespace=False) if t.strip()]

    HAVE_THAI = True
except Exception:  # pragma: no cover
    def thai_segment(text: str) -> List[str]:
        # Fallback: naive whitespace split (English-only meetings still work).
        return [t for t in text.split() if t.strip()]

    HAVE_THAI = False

try:
    import sqlite_vec  # type: ignore

    HAVE_VEC = True
except Exception:  # pragma: no cover
    HAVE_VEC = False


app = FastAPI(title="Meetily ML sidecar", version="1.0.0")


# ── Models ────────────────────────────────────────────────────────────────────
class TranslateReq(BaseModel):
    model: str = "translategemma:4b"
    text: str
    glossary: List[str] = []
    target: str = "en"  # "en" or "th"


class EmbedReq(BaseModel):
    model: str = "bge-m3"
    texts: List[str]


class SegmentReq(BaseModel):
    text: str


class IndexChunk(BaseModel):
    chunk_id: str
    text: str
    kind: str = "transcript"


class IndexReq(BaseModel):
    db_path: str
    embed_model: str = "bge-m3"
    meeting_date: str
    session_time: str
    topics: List[str] = []
    file_path: str
    summary_path: str = ""
    chunks: List[IndexChunk]


class SearchReq(BaseModel):
    db_path: str
    embed_model: str = "bge-m3"
    query: str
    limit: int = 10


# ── Ollama helpers ────────────────────────────────────────────────────────────
async def ollama_generate(model: str, prompt: str, system: Optional[str] = None) -> str:
    # `think: false` keeps qwen3 (and other thinking models) fast; ignored by others.
    body = {
        "model": model,
        "prompt": prompt,
        "stream": False,
        "think": False,
        "options": {"temperature": 0.1},
    }
    if system:
        body["system"] = system
    async with httpx.AsyncClient(timeout=120) as client:
        r = await client.post(f"{OLLAMA_URL}/api/generate", json=body)
        r.raise_for_status()
        return r.json().get("response", "").strip()


async def ollama_embed(model: str, texts: List[str]) -> List[List[float]]:
    async with httpx.AsyncClient(timeout=120) as client:
        r = await client.post(f"{OLLAMA_URL}/api/embed", json={"model": model, "input": texts})
        r.raise_for_status()
        data = r.json()
        return data.get("embeddings") or []


# ── DB helpers ────────────────────────────────────────────────────────────────
def open_db(db_path: str) -> sqlite3.Connection:
    os.makedirs(os.path.dirname(db_path), exist_ok=True)
    db = sqlite3.connect(db_path)
    db.row_factory = sqlite3.Row
    if HAVE_VEC:
        db.enable_load_extension(True)
        sqlite_vec.load(db)
        db.enable_load_extension(False)
    db.execute(
        """
        CREATE TABLE IF NOT EXISTS chunks(
            id INTEGER PRIMARY KEY,
            chunk_id TEXT UNIQUE,
            text TEXT,
            kind TEXT,
            meeting_date TEXT,
            session_time TEXT,
            topics TEXT,
            file_path TEXT,
            summary_path TEXT
        )
        """
    )
    db.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(text_seg, content='', tokenize='unicode61')"
    )
    if HAVE_VEC:
        db.execute(
            f"CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(embedding float[{EMBED_DIM}])"
        )
    db.commit()
    return db


# ── Endpoints ─────────────────────────────────────────────────────────────────
@app.get("/health")
async def health():
    return {
        "ok": True,
        "thai_segmentation": HAVE_THAI,
        "vector_search": HAVE_VEC,
        "ollama": OLLAMA_URL,
    }


@app.post("/translate")
async def translate(req: TranslateReq):
    if not req.text.strip():
        return {"translation": ""}
    glossary = ", ".join(req.glossary) if req.glossary else "Kafka, ACL, gRPC, consumer lag, Vault Core, Redis, partition, broker, deploy, UAT"
    target = (req.target or "en").lower()
    if target == "th":
        # Thai-language instruction is far more reliable for →Thai output.
        system = (
            "คุณเป็นนักแปลสำหรับการประชุมด้านวิศวกรรมซอฟต์แวร์ แปลจากภาษาอังกฤษ/ภาษาผสมเป็นภาษาไทย "
            f"คงคำศัพท์เทคนิคไว้เป็นภาษาอังกฤษเหมือนเดิม ({glossary}) "
            "ห้ามแปล ทับศัพท์ หรือดัดแปลงคำศัพท์เทคนิค ชื่อผลิตภัณฑ์ หรือชื่อตัวแปร/โค้ด — ให้คงเป็นภาษาอังกฤษ "
            "แปลส่วนที่เหลือทั้งหมด ตอบเป็นภาษาไทยเท่านั้น ห้ามมีเครื่องหมายคำพูด หมายเหตุ หรือข้อความอื่น"
        )
        prompt = f"แปลข้อความนี้เป็นภาษาไทย:\n{req.text}"
    else:
        system = (
            "You are a precise translator for software engineering meetings, translating into English. "
            f"Keep technical terms in English exactly as written ({glossary}). "
            "Do not translate, transliterate, or alter technical terms, product names, or code identifiers — "
            "leave them in English. Translate everything else. "
            "Return only the English translation, with no quotes, notes, or extra text."
        )
        prompt = f"Translate to English:\n{req.text}"
    try:
        out = await ollama_generate(req.model, prompt, system=system)
    except Exception as e:
        raise HTTPException(status_code=502, detail=f"ollama translate failed: {e}")
    # Strip accidental wrapping quotes/prefixes.
    out = out.strip().strip('"').strip()
    return {"translation": out}


@app.post("/embed")
async def embed(req: EmbedReq):
    try:
        vectors = await ollama_embed(req.model, req.texts)
    except Exception as e:
        raise HTTPException(status_code=502, detail=f"ollama embed failed: {e}")
    return {"embeddings": vectors}


@app.post("/segment")
async def segment(req: SegmentReq):
    return {"tokens": thai_segment(req.text), "engine": "newmm" if HAVE_THAI else "fallback"}


@app.post("/index")
async def index(req: IndexReq):
    if not req.chunks:
        return {"indexed": 0}
    texts = [c.text for c in req.chunks]

    embeddings: List[List[float]] = []
    if HAVE_VEC:
        try:
            embeddings = await ollama_embed(req.embed_model, texts)
        except Exception as e:
            # Indexing still useful via FTS even if embeddings fail.
            embeddings = []
            print(f"[index] embed failed, FTS-only: {e}")

    db = open_db(req.db_path)
    topics_str = ", ".join(req.topics)
    indexed = 0
    try:
        for i, chunk in enumerate(req.chunks):
            cur = db.execute(
                """
                INSERT INTO chunks(chunk_id, text, kind, meeting_date, session_time, topics, file_path, summary_path)
                VALUES(?,?,?,?,?,?,?,?)
                ON CONFLICT(chunk_id) DO UPDATE SET text=excluded.text
                RETURNING id
                """,
                (
                    chunk.chunk_id, chunk.text, chunk.kind, req.meeting_date,
                    req.session_time, topics_str, req.file_path, req.summary_path,
                ),
            )
            row = cur.fetchone()
            rowid = row["id"]

            seg = " ".join(thai_segment(chunk.text))
            db.execute("DELETE FROM chunks_fts WHERE rowid=?", (rowid,))
            db.execute("INSERT INTO chunks_fts(rowid, text_seg) VALUES(?,?)", (rowid, seg))

            if HAVE_VEC and i < len(embeddings) and embeddings[i]:
                db.execute("DELETE FROM vec_chunks WHERE rowid=?", (rowid,))
                db.execute(
                    "INSERT INTO vec_chunks(rowid, embedding) VALUES(?,?)",
                    (rowid, sqlite_vec.serialize_float32(embeddings[i])),
                )
            indexed += 1
        db.commit()
    finally:
        db.close()
    return {"indexed": indexed, "vector": HAVE_VEC and bool(embeddings)}


@app.post("/search")
async def search(req: SearchReq):
    if not os.path.exists(req.db_path):
        return {"results": []}
    db = open_db(req.db_path)
    try:
        k = max(req.limit * 3, 20)

        # Sparse (BM25 over segmented Thai/English)
        sparse_ranks = {}
        seg_query = " OR ".join(thai_segment(req.query)) or req.query
        try:
            rows = db.execute(
                "SELECT rowid, bm25(chunks_fts) AS score FROM chunks_fts WHERE chunks_fts MATCH ? ORDER BY score LIMIT ?",
                (seg_query, k),
            ).fetchall()
            for rank, r in enumerate(rows):
                sparse_ranks[r["rowid"]] = rank
        except Exception as e:
            print(f"[search] fts failed: {e}")

        # Dense (vector KNN)
        dense_ranks = {}
        if HAVE_VEC:
            try:
                qvec = (await ollama_embed(req.embed_model, [req.query]))[0]
                rows = db.execute(
                    "SELECT rowid, distance FROM vec_chunks WHERE embedding MATCH ? ORDER BY distance LIMIT ?",
                    (sqlite_vec.serialize_float32(qvec), k),
                ).fetchall()
                for rank, r in enumerate(rows):
                    dense_ranks[r["rowid"]] = rank
            except Exception as e:
                print(f"[search] vector failed: {e}")

        # Reciprocal Rank Fusion
        rrf = {}
        C = 60
        for rid, rank in sparse_ranks.items():
            rrf[rid] = rrf.get(rid, 0.0) + 1.0 / (C + rank)
        for rid, rank in dense_ranks.items():
            rrf[rid] = rrf.get(rid, 0.0) + 1.0 / (C + rank)

        ranked = sorted(rrf.items(), key=lambda kv: kv[1], reverse=True)[: req.limit]
        results = []
        for rid, score in ranked:
            row = db.execute("SELECT * FROM chunks WHERE id=?", (rid,)).fetchone()
            if not row:
                continue
            text = row["text"] or ""
            snippet = text.strip().replace("\n", " ")
            if len(snippet) > 240:
                snippet = snippet[:240] + "…"
            results.append({
                "snippet": snippet,
                "meeting_date": row["meeting_date"],
                "session_time": row["session_time"],
                "topics": [t.strip() for t in (row["topics"] or "").split(",") if t.strip()],
                "file_path": row["file_path"],
                "summary_path": row["summary_path"] or "",
                "score": round(float(score), 4),
            })
        return {"results": results}
    finally:
        db.close()


if __name__ == "__main__":
    import uvicorn

    port = int(os.environ.get("MEET_SIDECAR_PORT", "8178"))
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="info")
