// VALUEOS WS3: a bounded in-memory ring buffer of recent app log output. The app has no
// persistent log file (env_logger writes to stderr), so we patch console.* ONCE to also record
// lines here — giving a bug report recent logs to attach. Bounded by line count + chars so we
// never ship gigabytes. Content is scrubbed later (scrub.ts) before it can leave the machine.
const MAX_LINES = 2000;
const MAX_CHARS = 512 * 1024; // ~0.5 MB

let buffer: string[] = [];
let installed = false;

function safeStringify(v: unknown): string {
  if (typeof v === 'string') return v;
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

function record(level: string, args: unknown[]): void {
  try {
    const text = args.map(safeStringify).join(' ');
    buffer.push(`${new Date().toISOString()} [${level}] ${text}`);
    if (buffer.length > MAX_LINES) buffer = buffer.slice(-MAX_LINES);
  } catch {
    /* logging must never break the app */
  }
}

/** Patch console.* to also record into the ring buffer. Idempotent. */
export function installLogCapture(): void {
  if (installed || typeof console === 'undefined') return;
  installed = true;
  (['log', 'info', 'warn', 'error', 'debug'] as const).forEach((level) => {
    const orig = console[level]?.bind(console);
    if (!orig) return;
    console[level] = (...args: unknown[]) => {
      record(level, args);
      orig(...args);
    };
  });
}

/** Directly append a line (used by tests / explicit valueos logging). */
export function pushLog(line: string): void {
  record('log', [line]);
}

export function getRecentLogs(maxChars = MAX_CHARS): string {
  const joined = buffer.join('\n');
  return joined.length > maxChars ? joined.slice(joined.length - maxChars) : joined;
}

export function clearLogCapture(): void {
  buffer = [];
}
