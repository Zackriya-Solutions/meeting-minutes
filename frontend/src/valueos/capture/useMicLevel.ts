'use client';
// VALUEOS: a REAL VU meter from the webview's OWN microphone (getUserMedia + Web Audio),
// independent of the Rust capture — a genuine "it's hearing you" live signal during capture.
// DISPLAY-ONLY: this audio is never recorded, saved, or uploaded; the stream + AudioContext are
// released when capture stops (or the component unmounts). Updates are throttled to ~15 Hz so
// they don't thrash rendering. Returns a level in [0,1], or -1 when unavailable (no permission /
// unsupported) so the UI can fall back to an animated indicator.
import { useEffect, useState } from 'react';

/** RMS of time-domain PCM (Uint8, 128 = silence), scaled for visibility. Pure → testable. */
export function rmsFromTimeDomain(data: Uint8Array): number {
  if (data.length === 0) return 0;
  let sum = 0;
  for (let i = 0; i < data.length; i++) {
    const v = (data[i] - 128) / 128;
    sum += v * v;
  }
  const rms = Math.sqrt(sum / data.length);
  return Math.max(0, Math.min(1, rms * 3));
}

export function useMicLevel(active: boolean): number {
  const [level, setLevel] = useState(0);

  useEffect(() => {
    if (!active) {
      setLevel(0);
      return;
    }
    if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia || typeof AudioContext === 'undefined') {
      setLevel(-1); // unavailable → UI falls back to the animated indicator
      return;
    }

    let stream: MediaStream | null = null;
    let ctx: AudioContext | null = null;
    let raf = 0;
    let stopped = false;
    let lastTick = 0;

    (async () => {
      try {
        stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        if (stopped) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        ctx = new AudioContext();
        const src = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 512;
        src.connect(analyser);
        const data = new Uint8Array(analyser.frequencyBinCount);
        const tick = (t: number) => {
          if (stopped) return;
          if (t - lastTick >= 66) {
            // ~15 Hz
            lastTick = t;
            analyser.getByteTimeDomainData(data);
            setLevel(rmsFromTimeDomain(data));
          }
          raf = requestAnimationFrame(tick);
        };
        raf = requestAnimationFrame(tick);
      } catch {
        setLevel(-1);
      }
    })();

    return () => {
      stopped = true;
      if (raf) cancelAnimationFrame(raf);
      stream?.getTracks().forEach((t) => t.stop());
      ctx?.close().catch(() => {});
      setLevel(0);
    };
  }, [active]);

  return level;
}
