// VALUEOS: the "digest" = a human-readable HIGH-LEVEL RECAP of the transcript (NOT a hash).
// Interface + a mock generator. Phase 3, the real generator reuses upstream local
// summarization (crate::summary::llm_client::generate_summary via a thin valueos command,
// or api_process_transcript) behind this same interface — screens/finalize depend only on
// the interface.
export interface DigestOptions {
  title?: string;
  maxChars?: number;
}

export interface DigestGenerator {
  /** Turn transcript text into a short readable recap. */
  generate(transcript: string, opts?: DigestOptions): Promise<string>;
}

/**
 * ⚠️ MOCK generator — a deterministic extractive recap (lead sentences + a one-line
 * overview). Readable prose, never a hash. Good enough for tests and offline dev; the
 * real model-backed recap replaces it in Phase 3 behind this interface.
 */
export class MockDigestGenerator implements DigestGenerator {
  async generate(transcript: string, opts?: DigestOptions): Promise<string> {
    const clean = transcript.replace(/\s+/g, ' ').trim();
    if (!clean) return `${opts?.title ? opts.title + ' — ' : ''}No speech was captured.`;
    const sentences = clean.split(/(?<=[.!?])\s+/).filter(Boolean);
    const words = clean.split(' ').length;
    const lead = sentences.slice(0, 5).join(' ');
    const header = opts?.title ? `Recap — ${opts.title}` : 'Meeting recap';
    const body = `Overview: a ${words}-word conversation.\n\nKey points:\n${lead}`;
    const out = `${header}\n\n${body}`;
    const max = opts?.maxChars ?? 4000;
    return out.length > max ? out.slice(0, max - 1) + '…' : out;
  }
}
