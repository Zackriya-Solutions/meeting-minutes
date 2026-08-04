"use client";

import { useEffect, useMemo, useState } from "react";

const DEFAULT_ROTATION_INTERVAL_MS = 5_000;

export function useRotatingPlaceholder(
  suggestions: readonly string[],
  fallback: string,
  intervalMs = DEFAULT_ROTATION_INTERVAL_MS,
): string {
  const normalized = useMemo(
    () => Array.from(new Set(suggestions.map((value) => value.trim()).filter(Boolean))),
    [suggestions],
  );
  const signature = normalized.join("\u0000");
  const [index, setIndex] = useState(0);

  useEffect(() => {
    setIndex(0);
  }, [signature]);

  useEffect(() => {
    if (normalized.length < 2) return;

    const timer = window.setInterval(() => {
      setIndex((current) => (current + 1) % normalized.length);
    }, intervalMs);

    return () => window.clearInterval(timer);
  }, [intervalMs, normalized.length, signature]);

  return normalized[index % Math.max(normalized.length, 1)] ?? fallback;
}
