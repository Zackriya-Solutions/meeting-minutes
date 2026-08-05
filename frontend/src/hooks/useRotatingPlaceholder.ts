"use client";

import { useEffect, useMemo, useState } from "react";

const DEFAULT_ROTATION_INTERVAL_MS = 5_000;

interface SharedRotationState {
  signature: string;
  index: number;
}

// Route drawers render a second copy of the screen behind the panel. A keyed
// rotation survives that client-side remount, so opening a drawer cannot act
// like an implicit "next suggestion" command.
const sharedRotations = new Map<string, SharedRotationState>();

function sharedRotationIndex(key: string, signature: string, length: number): number {
  const existing = sharedRotations.get(key);
  if (existing?.signature === signature) return existing.index % Math.max(length, 1);

  sharedRotations.set(key, { signature, index: 0 });
  return 0;
}

export function useRotatingPlaceholder(
  suggestions: readonly string[],
  fallback: string,
  intervalMs = DEFAULT_ROTATION_INTERVAL_MS,
  rotationKey?: string,
): string {
  const normalized = useMemo(
    () => Array.from(new Set(suggestions.map((value) => value.trim()).filter(Boolean))),
    [suggestions],
  );
  const signature = normalized.join("\u0000");
  const [index, setIndex] = useState(() =>
    rotationKey ? sharedRotationIndex(rotationKey, signature, normalized.length) : 0,
  );

  useEffect(() => {
    setIndex(rotationKey ? sharedRotationIndex(rotationKey, signature, normalized.length) : 0);
  }, [normalized.length, rotationKey, signature]);

  useEffect(() => {
    if (normalized.length < 2) return;

    const timer = window.setInterval(() => {
      setIndex((current) => {
        const currentIndex = rotationKey
          ? sharedRotationIndex(rotationKey, signature, normalized.length)
          : current;
        const nextIndex = (currentIndex + 1) % normalized.length;
        if (rotationKey) sharedRotations.set(rotationKey, { signature, index: nextIndex });
        return nextIndex;
      });
    }, intervalMs);

    return () => window.clearInterval(timer);
  }, [intervalMs, normalized.length, rotationKey, signature]);

  return normalized[index % Math.max(normalized.length, 1)] ?? fallback;
}
