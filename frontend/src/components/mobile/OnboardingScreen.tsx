'use client';

import { useCallback, useEffect, useState } from 'react';
import { Check, Mic } from 'lucide-react';
import type { VmModel } from './types';
import {
  downloadModel,
  onModelDownloadComplete,
  onModelDownloadProgress,
  requestMicPermission,
} from './tauriBridge';

const RECOMMENDED = 'base';

function Shield({ variant }: { variant: 'check' | 'info' }) {
  return (
    <svg className="shield" viewBox="0 0 64 74" fill="none">
      <path
        d="M32 2L60 14V34C60 54 46 66 32 72C18 66 4 54 4 34V14L32 2Z"
        fill="hsl(var(--accent))"
        stroke="hsl(var(--primary))"
        strokeWidth="2.5"
      />
      {variant === 'check' ? (
        <path
          d="M22 36L29 43L44 27"
          stroke="hsl(var(--primary))"
          strokeWidth="4"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      ) : (
        <path
          d="M32 24V40M32 48H32.01"
          stroke="hsl(var(--primary))"
          strokeWidth="4"
          strokeLinecap="round"
        />
      )}
    </svg>
  );
}

export function OnboardingScreen({
  models,
  onFinished,
}: {
  models: VmModel[];
  onFinished: () => void;
}) {
  const [step, setStep] = useState(0);
  const [micDenied, setMicDenied] = useState(false);
  const [requestingMic, setRequestingMic] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [downloaded, setDownloaded] = useState(false);

  const recommendedModel =
    models.find((m) => m.name.toLowerCase() === RECOMMENDED) ??
    models.find((m) => m.recommended);
  const alreadyHasModel =
    downloaded || models.some((m) => m.status === 'downloaded');

  useEffect(() => {
    let unProgress: (() => void) | undefined;
    let unComplete: (() => void) | undefined;
    onModelDownloadProgress((_name, p) => setProgress(p)).then((u) => (unProgress = u));
    onModelDownloadComplete(() => {
      setDownloading(false);
      setDownloaded(true);
      setProgress(100);
      setStep(3);
    }).then((u) => (unComplete = u));
    return () => {
      unProgress?.();
      unComplete?.();
    };
  }, []);

  const allowMic = useCallback(async () => {
    setRequestingMic(true);
    const granted = await requestMicPermission();
    setRequestingMic(false);
    setMicDenied(!granted);
    if (granted) setStep(2);
  }, []);

  const next = useCallback(async () => {
    if (step === 2) {
      if (alreadyHasModel) {
        setStep(3);
        return;
      }
      setDownloading(true);
      setProgress(0);
      try {
        await downloadModel(recommendedModel?.name ?? RECOMMENDED);
      } catch (e) {
        console.warn('[vm] onboarding model download failed', e);
        setDownloading(false);
      }
      return;
    }
    if (step === 3) {
      onFinished();
      return;
    }
    setStep((s) => Math.min(s + 1, 3));
  }, [step, alreadyHasModel, recommendedModel, onFinished]);

  const primaryLabel =
    step === 0
      ? 'Continue'
      : step === 2
        ? downloading
          ? 'Downloading…'
          : alreadyHasModel
            ? 'Continue'
            : 'Download & continue'
        : step === 3
          ? 'Get started'
          : 'Continue';

  const dot = (i: number) =>
    step === i ? 'hsl(var(--primary))' : 'hsl(var(--border))';

  return (
    <div className="col f1" style={{ height: '100%' }}>
      <div className="row between" style={{ padding: '16px 20px 0' }}>
        <div className="row gap8">
          <div style={{ width: 22, height: 4, borderRadius: 2, background: 'hsl(var(--primary))' }} />
          <span className="mono fs11 fw7" style={{ letterSpacing: '0.06em' }}>
            VOICE ME
          </span>
        </div>
        {step < 3 && (
          <button
            className="btn"
            style={{ background: 'none', color: 'hsl(var(--muted-fg))', fontSize: 12, textDecoration: 'underline', textUnderlineOffset: 2 }}
            onClick={onFinished}
          >
            Skip
          </button>
        )}
      </div>

      {step === 0 && (
        <div className="col ac jc f1" style={{ padding: '32px 28px', textAlign: 'center', gap: 22 }}>
          <Shield variant="check" />
          <h1 style={{ fontSize: 26, fontWeight: 800, margin: 0, letterSpacing: '-0.02em' }}>
            Your meetings never leave this device
          </h1>
          <p className="muted" style={{ fontSize: 15, lineHeight: 1.5, margin: 0 }}>
            Voice Me records, transcribes, and summarizes locally. No cloud accounts. No telemetry.
            Open source, start to finish.
          </p>
        </div>
      )}

      {step === 1 && (
        <div className="col ac jc f1" style={{ padding: '32px 28px', textAlign: 'center', gap: 20 }}>
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
            <Mic size={30} color="hsl(var(--primary))" />
          </div>
          <h1 style={{ fontSize: 22, fontWeight: 800, margin: 0 }}>Microphone access</h1>
          <p className="muted" style={{ fontSize: 15, lineHeight: 1.5, margin: 0 }}>
            Voice Me needs the mic to capture audio for local transcription. Audio is processed
            with an on-device Whisper model and is never sent anywhere.
          </p>
          {micDenied && (
            <div
              className="card"
              style={{
                padding: 14,
                textAlign: 'left',
                background: 'hsl(var(--destructive)/0.08)',
                borderColor: 'hsl(var(--destructive)/0.3)',
              }}
            >
              <p style={{ margin: 0, fontSize: 13, color: 'hsl(var(--destructive))' }}>
                Microphone access is off. Recording is disabled until you allow it in system
                settings.
              </p>
            </div>
          )}
        </div>
      )}

      {step === 2 && (
        <div className="col f1" style={{ padding: '28px 24px', gap: 18 }}>
          <h1 style={{ fontSize: 22, fontWeight: 800, margin: 0 }}>Download a speech model</h1>
          <p className="muted" style={{ fontSize: 14, lineHeight: 1.5, margin: 0 }}>
            Whisper runs fully on-device. We recommend Base for this device — you can add more
            later in Model Manager.
          </p>
          <div className="card" style={{ padding: 16 }}>
            <div className="row between">
              <div className="row gap12">
                <div
                  style={{
                    width: 38,
                    height: 38,
                    borderRadius: 10,
                    background: 'hsl(var(--accent))',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontWeight: 800,
                    color: 'hsl(var(--primary))',
                    fontSize: 13,
                  }}
                >
                  B
                </div>
                <div className="col">
                  <span style={{ fontWeight: 700, fontSize: 15 }}>
                    {recommendedModel?.name ?? 'Base'}
                  </span>
                  <span className="muted" style={{ fontSize: 12.5 }}>
                    {recommendedModel ? `${recommendedModel.size_mb} MB` : '142 MB'} · Recommended
                  </span>
                </div>
              </div>
              <span className="pill" style={{ background: 'hsl(var(--accent))', color: 'hsl(var(--accent-fg))' }}>
                Recommended
              </span>
            </div>
            {downloading && (
              <div style={{ marginTop: 14 }}>
                <div className="progress-track">
                  <div className="progress-fill" style={{ width: `${progress}%` }} />
                </div>
                <span className="mono muted" style={{ fontSize: 12, display: 'block', marginTop: 6 }}>
                  {progress}%
                </span>
              </div>
            )}
          </div>
        </div>
      )}

      {step === 3 && (
        <div className="col ac jc f1" style={{ padding: '32px 28px', textAlign: 'center', gap: 20 }}>
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
            <Check size={30} color="hsl(var(--primary))" strokeWidth={2.5} />
          </div>
          <h1 style={{ fontSize: 22, fontWeight: 800, margin: 0 }}>You&apos;re all set</h1>
          <p className="muted" style={{ fontSize: 15, lineHeight: 1.5, margin: 0 }}>
            Start your first recording whenever you like — everything stays on this device.
          </p>
        </div>
      )}

      <div className="col gap12" style={{ padding: '12px 24px 26px' }}>
        <div className="row jc gap8" style={{ padding: '6px 0' }}>
          {[0, 1, 2, 3].map((i) => (
            <span key={i} style={{ width: 7, height: 7, borderRadius: '50%', background: dot(i) }} />
          ))}
        </div>
        {step === 1 ? (
          <>
            <button className="btn btnp lg" onClick={allowMic} disabled={requestingMic}>
              {requestingMic ? 'Requesting…' : 'Allow microphone access'}
            </button>
            <button
              className="btn btnghost md"
              style={{ width: '100%' }}
              onClick={() => {
                setMicDenied(true);
                setStep(2);
              }}
            >
              Not now
            </button>
          </>
        ) : (
          <button className="btn btnp lg" onClick={next} disabled={downloading}>
            {primaryLabel}
          </button>
        )}
      </div>
    </div>
  );
}
