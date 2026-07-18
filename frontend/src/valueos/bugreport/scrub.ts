// VALUEOS WS3: the single scrubbing pass. Runs over the WHOLE bundle (logs + description)
// BEFORE anything leaves the machine. Fail-closed: redact aggressively; when unsure, redact.
// Guarantees (tested): no auth material (tokens, Bearer headers, OAuth code/PKCE, presigned
// URLs, api keys) and no obvious PII (emails) survive; known transcript-content log lines are
// dropped. Raw transcript text / audio are NEVER put in the bundle in the first place.
import type { BugReportBundle } from './types';

const REDACTED = '[REDACTED]';

// Order matters — specific (auth) before generic (email).
const RULES: Array<[RegExp, string]> = [
  // JWTs (three base64url segments) — covers access/id tokens wherever they appear.
  [/eyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}/g, '[REDACTED_JWT]'],
  // Authorization: Bearer <token>  (header or log)
  [/(authorization["']?\s*[:=]\s*["']?)(bearer\s+)?[A-Za-z0-9._~+/=-]{8,}/gi, `$1$2${REDACTED}`],
  // sensitive values by key, in JSON or key=value form
  [
    /(["']?(?:access_token|refresh_token|id_token|token|code|code_verifier|code_challenge|client_secret|x-api-key|api_key|apikey|password|secret|authorization)["']?\s*[:=]\s*["'])([^"']*)(["'])/gi,
    `$1${REDACTED}$3`,
  ],
  [
    /\b(access_token|refresh_token|id_token|token|code_verifier|code_challenge|x-api-key|api_key|apikey|password|secret|code)=([^&\s"']+)/gi,
    `$1=${REDACTED}`,
  ],
  // Presigned URLs (S3/CloudFront) — redact the whole query string.
  [
    /(https?:\/\/[^\s"'?]+)\?[^\s"']*(?:x-amz-signature|x-amz-credential|awsaccesskeyid|x-amz-security-token|signature)=[^\s"']*/gi,
    '$1?[REDACTED_PRESIGNED]',
  ],
  // Emails (PII)
  [/\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/g, '[EMAIL]'],
];

/** Log lines whose CONTENT is likely captured speech — drop them entirely (fail-closed). */
const TRANSCRIPT_LINE = /(transcript|transcribed|latest transcript|addtranscript|raw_content|utterance)/i;

/** Scrub a free-text blob (auth material + PII). */
export function scrubText(input: string): string {
  let out = input ?? '';
  for (const [re, repl] of RULES) out = out.replace(re, repl);
  return out;
}

/** Scrub logs: drop transcript-content lines, then scrub the rest. */
export function scrubLogs(logs: string): string {
  const kept = (logs ?? '')
    .split('\n')
    .map((line) => (TRANSCRIPT_LINE.test(line) ? '[line omitted: may contain transcript content]' : line));
  return scrubText(kept.join('\n'));
}

/** Scrub the whole bundle in one pass (logs + description). Metadata is structured + safe. */
export function scrubBundle(bundle: BugReportBundle): BugReportBundle {
  return {
    ...bundle,
    description: scrubText(bundle.description),
    scrubbed_logs: scrubLogs(bundle.scrubbed_logs),
  };
}
