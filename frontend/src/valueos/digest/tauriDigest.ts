// VALUEOS: REAL digest — a native command that reuses upstream local summarization
// (crate::summary::llm_client::generate_summary) to turn the transcript into a readable
// high-level recap. Same interface as the mock.
import { DigestGenerator, DigestOptions } from './digest';
import { callValueOs } from '../transport/invoke';

export function createTauriDigest(): DigestGenerator {
  return {
    generate(transcript: string, opts?: DigestOptions) {
      return callValueOs<string>('valueos_generate_digest', {
        transcript,
        title: opts?.title,
        max_chars: opts?.maxChars,
      });
    },
  };
}
