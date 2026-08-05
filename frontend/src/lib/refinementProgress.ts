/**
 * Stage labelling for the post-meeting refinement pass.
 *
 * Kept apart from the hook that subscribes to the events so it can be tested without a DOM:
 * this is the part with actual decisions in it (which label, when to append a count, what to
 * show before the first event arrives), and the part a new stage in the Rust pass would break.
 */

/** Stages emitted by the Rust refinement pass (`audio/refinement.rs`). */
export type RefinementStage =
    | 'waiting_for_model'
    | 'diarizing'
    | 'decoding'
    | 'transcribing'
    | 'attributing'
    | 'retranscribing'
    | 'exporting';

export interface RefinementProgressPayload {
    meeting_id: string;
    stage: RefinementStage;
    done: number;
    total: number;
}

/** English keys for the i18n dictionary; `t()` maps them to the active language. */
const STAGE_KEYS: Record<RefinementStage, string> = {
    waiting_for_model: 'Waiting for the speech model',
    diarizing: 'Separating voices',
    decoding: 'Reading the recording',
    transcribing: 'Splitting replies',
    attributing: 'Labelling speakers',
    retranscribing: 'Re-transcribing',
    exporting: 'Saving',
};

/**
 * The chip's text, or null when nothing is running.
 *
 * @param running Whether a pass is in flight.
 * @param stage Last stage reported, or null if none has arrived yet.
 * @param progress Span counts, only meaningful while transcribing.
 * @param t Translator.
 */
export function refinementLabel(
    running: boolean,
    stage: RefinementStage | null,
    progress: { done: number; total: number } | null,
    t: (key: string) => string,
): string | null {
    if (!running) return null;
    // A pass can already be under way when the meeting is opened, so the running flag can
    // precede the first stage event by minutes — say something rather than nothing.
    if (!stage) return t('Processing');
    const label = t(STAGE_KEYS[stage]);
    // Only the per-turn ASR loop has a countable unit of work; a bare "0/0" would be worse
    // than no counter at all.
    if (stage === 'transcribing' && progress && progress.total > 0) {
        return `${label} ${progress.done}/${progress.total}`;
    }
    return label;
}
