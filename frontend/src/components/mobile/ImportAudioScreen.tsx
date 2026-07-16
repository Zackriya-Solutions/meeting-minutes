'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronLeft, FileAudio, Upload } from 'lucide-react';
import type { VmModel } from './types';
import {
  AudioFileInfo,
  cancelAudioImport,
  onImportComplete,
  onImportError,
  onImportProgress,
  onImportWarning,
  pickAndValidateAudioFile,
  startAudioImport,
} from './tauriBridge';

type Stage = 'idle' | 'picking' | 'ready' | 'importing' | 'error' | 'done';

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${bytes} B`;
}

function formatDuration(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  return `${m}m ${s}s`;
}

export function ImportAudioScreen({
  models,
  onBack,
  onImported,
}: {
  models: VmModel[];
  onBack: () => void;
  onImported: (meetingId: string) => void;
}) {
  const [stage, setStage] = useState<Stage>('idle');
  const [file, setFile] = useState<AudioFileInfo | null>(null);
  const [title, setTitle] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  const [progressMessage, setProgressMessage] = useState('');

  const downloadedModel = models.find((m) => m.status === 'downloaded');
  const cancelledRef = useRef(false);

  useEffect(() => {
    let unProgress: (() => void) | undefined;
    let unComplete: (() => void) | undefined;
    let unError: (() => void) | undefined;
    let unWarning: (() => void) | undefined;

    onImportProgress((e) => {
      setProgress(e.progress_percentage);
      setProgressMessage(e.message);
    }).then((u) => (unProgress = u));

    onImportComplete((e) => {
      setStage('done');
      if (!cancelledRef.current) onImported(e.meeting_id);
    }).then((u) => (unComplete = u));

    onImportError((err) => {
      setError(err);
      setStage('error');
    }).then((u) => (unError = u));

    onImportWarning((w) => setWarning(w)).then((u) => (unWarning = u));

    return () => {
      unProgress?.();
      unComplete?.();
      unError?.();
      unWarning?.();
    };
  }, [onImported]);

  const pickFile = useCallback(async () => {
    setStage('picking');
    setError(null);
    try {
      const info = await pickAndValidateAudioFile();
      if (!info) {
        // User cancelled the file picker
        setStage('idle');
        return;
      }
      setFile(info);
      setTitle(info.filename.replace(/\.[^/.]+$/, ''));
      setStage('ready');
    } catch (e) {
      setError(String(e));
      setStage('error');
    }
  }, []);

  const beginImport = useCallback(async () => {
    if (!file || !downloadedModel) return;
    setStage('importing');
    setProgress(0);
    setProgressMessage('Starting…');
    try {
      await startAudioImport(file.path, title.trim() || file.filename, downloadedModel.name);
    } catch (e) {
      setError(String(e));
      setStage('error');
    }
  }, [file, title, downloadedModel]);

  const cancel = useCallback(async () => {
    cancelledRef.current = true;
    await cancelAudioImport();
    onBack();
  }, [onBack]);

  return (
    <div className="col f1" style={{ height: '100%' }}>
      <div className="appbar" style={{ padding: '8px 6px 0' }}>
        <button className="iconbtn" onClick={stage === 'importing' ? cancel : onBack}>
          <ChevronLeft size={22} strokeWidth={2.2} />
        </button>
        <h1>Import audio</h1>
      </div>

      <div className="content" style={{ padding: '16px 20px 24px' }}>
        {!downloadedModel && (
          <div className="card col gap10" style={{ padding: 18, textAlign: 'center' }}>
            <span style={{ fontWeight: 700, fontSize: 15 }}>Download a speech model first</span>
            <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5 }}>
              Importing audio transcribes it on-device with Whisper, the same as live recording.
            </p>
          </div>
        )}

        {downloadedModel && stage === 'idle' && (
          <div className="col ac jc gap16" style={{ padding: '40px 12px', textAlign: 'center' }}>
            <div
              style={{
                width: 72,
                height: 72,
                borderRadius: '50%',
                background: 'hsl(var(--accent))',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Upload size={30} color="hsl(var(--primary))" />
            </div>
            <div className="col gap6">
              <span style={{ fontWeight: 800, fontSize: 17 }}>Transcribe a recording</span>
              <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5, maxWidth: 260 }}>
                Pick an audio file already on your device. It's transcribed locally, the same as
                a live recording.
              </p>
            </div>
            <button className="btn btnp md" onClick={pickFile}>
              Choose file
            </button>
          </div>
        )}

        {stage === 'picking' && (
          <div className="col ac jc" style={{ padding: '60px 12px' }}>
            <p className="muted fs13">Opening file picker…</p>
          </div>
        )}

        {stage === 'ready' && file && (
          <div className="col gap16">
            <div className="card row gap12" style={{ padding: 16 }}>
              <div
                style={{
                  width: 40,
                  height: 40,
                  borderRadius: 10,
                  background: 'hsl(var(--accent))',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  flexShrink: 0,
                }}
              >
                <FileAudio size={18} color="hsl(var(--primary))" />
              </div>
              <div className="col f1" style={{ minWidth: 0, gap: 2 }}>
                <span
                  style={{
                    fontWeight: 700,
                    fontSize: 14,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {file.filename}
                </span>
                <span className="muted mono fs12">
                  {formatDuration(file.duration_seconds)} · {formatBytes(file.size_bytes)} ·{' '}
                  {file.format.toUpperCase()}
                </span>
              </div>
            </div>

            <div className="col gap8">
              <span className="muted fw7 fs13">Meeting title</span>
              <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Title" />
            </div>

            <button className="btn btnp lg" onClick={beginImport}>
              Transcribe
            </button>
            <button className="btn btns md" onClick={pickFile}>
              Choose a different file
            </button>
          </div>
        )}

        {stage === 'importing' && (
          <div className="card col ac jc gap14" style={{ padding: '32px 20px' }}>
            <div className="mono fs13 muted">{progressMessage || 'Working…'}</div>
            <div className="progress-track" style={{ width: '100%' }}>
              <div className="progress-fill" style={{ width: `${progress}%` }} />
            </div>
            <span className="mono muted fs12">{progress}%</span>
            <button className="btn btns md" onClick={cancel}>
              Cancel
            </button>
          </div>
        )}

        {stage === 'error' && (
          <div className="card col ac gap10" style={{ padding: '26px 20px', textAlign: 'center' }}>
            <span style={{ fontWeight: 700, fontSize: 15 }}>Import failed</span>
            <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5 }}>{error}</p>
            <button className="btn btnp md" onClick={pickFile}>
              Try again
            </button>
          </div>
        )}

        {warning && stage !== 'error' && (
          <p className="muted fs12" style={{ marginTop: 14 }}>
            {warning}
          </p>
        )}
      </div>
    </div>
  );
}
