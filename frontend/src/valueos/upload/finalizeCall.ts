// VALUEOS: the shared finalize step — write the transcript file, generate the high-level
// digest, and upload the call + transcript + digest in ONE atomic op (composite /calls) via
// the resilient pending-upload queue, then record it in local history. Extracted from the
// old FinalizeScreen so the redesigned Recording screen ("End & upload") can call it
// directly. Idempotency key is stable across retries → no duplicates.
import type { ValueOsServices } from '../context/ValueOsProvider';
import type { CreateCallRequest } from '../api/types';
import type { CaptureResult } from '../shell/flowTypes';
import type { TranscriptRecord } from '../history/transcriptHistory';

export type FinalizeStatus = 'done' | 'pending' | 'reauth' | 'deEntitled' | 'error';

export interface FinalizeOutcome {
  status: FinalizeStatus;
  detail?: string;
  /** The history record written for this call (always written — the file is on disk
   *  regardless of upload result, so nothing is ever lost). */
  record: TranscriptRecord;
}

function sanitize(s: string): string {
  return s.replace(/[^a-z0-9]+/gi, '-').toLowerCase().slice(0, 40) || 'meeting';
}

export function newIdempotencyKey(): string {
  return globalThis.crypto?.randomUUID?.() ?? `id-${Math.random().toString(36).slice(2)}`;
}

export async function finalizeCall(
  services: Pick<ValueOsServices, 'digest' | 'config' | 'uploadQueue' | 'history'>,
  capture: CaptureResult,
  idempotencyKey: string = newIdempotencyKey(),
  now: () => number = () => Date.now(),
): Promise<FinalizeOutcome> {
  const { digest, config, uploadQueue, history } = services;
  const key = idempotencyKey;

  const digestText = await digest.generate(capture.transcriptText, { title: capture.callName });
  const fileName = `${sanitize(capture.callName)}-${key.slice(0, 8)}.txt`;
  const path = await config.writeTranscriptFile(fileName, capture.transcriptText);

  // PRIMARY write: the composite /calls — creates the call AND attaches the transcript +
  // locally-generated digest in one atomic op. Link is XOR in the body.
  const request: CreateCallRequest = {
    name: capture.callName,
    ...(capture.activityType === 'lead'
      ? { lead_id: capture.targetId }
      : { opportunity_id: capture.targetId }),
    transcript: {
      raw_content: capture.transcriptText,
      digest: digestText,
      file_name: fileName,
      title: capture.callName,
    },
    idempotency_key: key,
  };

  await uploadQueue.enqueue({ id: key, tenantId: capture.tenantId, request, transcriptPath: path });
  const outcome = await uploadQueue.flush();
  const uploaded = outcome.uploaded.includes(key);
  const failed = outcome.failed.find((f) => f.id === key);
  const lostAccess = outcome.deEntitled.includes(capture.tenantId);

  const record: TranscriptRecord = {
    id: key,
    targetLabel: capture.targetLabel,
    tenantId: capture.tenantId,
    tenantName: capture.tenantName,
    activityType: capture.activityType,
    targetId: capture.targetId,
    createdAt: now(),
    path,
    uploadStatus: uploaded ? 'uploaded' : failed || lostAccess ? 'failed' : 'pending',
    digest: digestText,
    transcript: capture.transcriptText,
  };
  await history.add(record);

  let status: FinalizeStatus;
  let detail: string | undefined;
  if (outcome.needsReauth) status = 'reauth';
  else if (lostAccess) status = 'deEntitled';
  else if (failed) {
    status = 'error';
    detail = `Upload failed: ${failed.message}`;
  } else if (outcome.retained.includes(key)) {
    status = 'pending';
    detail = 'Saved locally — will retry the upload.';
  } else status = 'done';

  return { status, detail, record };
}
