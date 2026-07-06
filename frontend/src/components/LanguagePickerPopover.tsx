"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { LANGUAGE_OPTIONS } from "@/lib/summary-languages";
import { useRecentLanguages } from "@/hooks/useRecentLanguages";

interface LanguagePickerPopoverProps {
  value: string | null;
  onChange: (code: string | null) => void;
  onClose: () => void;
  mode?: "meeting" | "settings";
  autoSubtitle?: string;
}

export function LanguagePickerPopover({
  value,
  onChange,
  onClose,
  mode = "meeting",
  autoSubtitle,
}: LanguagePickerPopoverProps) {
  const { recents } = useRecentLanguages();
  const [query, setQuery] = useState("");
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const onDocClick = (e: MouseEvent) => {
      if (!containerRef.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const filter = query.trim().toLowerCase();

  const recentCodes = useMemo(() => new Set(recents), [recents]);

  const filteredAll = useMemo(() => {
    const options = mode === "meeting"
      ? LANGUAGE_OPTIONS.filter((l) => !recentCodes.has(l.code))
      : LANGUAGE_OPTIONS;
    if (!filter) return options;
    return options.filter(
      (l) =>
        l.code.toLowerCase().includes(filter) ||
        l.label.toLowerCase().includes(filter),
    );
  }, [filter, mode, recentCodes]);

  const recentsResolved = useMemo(
    () =>
      recents
        .map((code) => LANGUAGE_OPTIONS.find((l) => l.code === code))
        .filter((l): l is (typeof LANGUAGE_OPTIONS)[number] => Boolean(l))
        .filter(
          (l) =>
            !filter ||
            l.code.toLowerCase().includes(filter) ||
            l.label.toLowerCase().includes(filter),
        ),
    [recents, filter],
  );

  const showAuto = mode === "meeting" && (!filter || "auto".includes(filter));
  const showRecents = mode === "meeting" && recentsResolved.length > 0;
  const hasNoResults =
    filteredAll.length === 0 && recentsResolved.length === 0 && !showAuto;

  return (
    <div
      ref={containerRef}
      className="w-72 rounded-lg bg-card border border-border shadow-lg overflow-hidden"
      role="dialog"
      aria-label="Pick summary language"
    >
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border">
        <span className="text-muted-foreground text-sm">🔍</span>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search language..."
          className="flex-1 text-sm text-foreground bg-transparent border-none outline-none placeholder-muted-foreground"
        />
      </div>

      <div className="max-h-80 overflow-y-auto py-1">
        {showRecents && (
          <>
            <div className="px-3 pt-1 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
              Recently Used
            </div>
            {recentsResolved.map((opt) => (
              <button
                key={`recent-${opt.code}`}
                type="button"
                aria-pressed={value === opt.code}
                onClick={() => onChange(opt.code)}
                className={`flex w-full items-center justify-between px-3 py-1.5 text-sm hover:bg-muted/40 text-left ${
                  value === opt.code ? "text-info-text font-medium" : "text-foreground"
                }`}
              >
                <span>
                  {opt.label}{" "}
                  <span className="text-xs text-muted-foreground">({opt.code})</span>
                </span>
                {value === opt.code && <span className="text-info-text" aria-hidden="true">✓</span>}
              </button>
            ))}
            <div className="my-1 h-px bg-muted" />
          </>
        )}

        {showAuto && (
          <button
            type="button"
            aria-pressed={value === null}
            onClick={() => onChange(null)}
            className={`flex w-full items-center justify-between px-3 py-1.5 text-sm hover:bg-muted/40 text-left ${
              value === null ? "text-info-text font-medium" : "text-foreground"
            }`}
          >
            <span className="flex flex-col">
              <span>Auto</span>
              {autoSubtitle && (
                <span className="text-xs font-normal text-muted-foreground">{autoSubtitle}</span>
              )}
            </span>
            {value === null && <span className="text-info-text" aria-hidden="true">✓</span>}
          </button>
        )}

        {filteredAll.length > 0 && (
          <div className="px-3 pt-1 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {mode === "meeting" ? "Other Languages" : "All Languages"}
          </div>
        )}

        {filteredAll.map((opt) => (
          <button
            key={`all-${opt.code}`}
            type="button"
            aria-pressed={value === opt.code}
            onClick={() => onChange(opt.code)}
            className={`flex w-full items-center justify-between px-3 py-1.5 text-sm hover:bg-muted/40 text-left ${
              value === opt.code ? "text-info-text font-medium" : "text-foreground"
            }`}
          >
            <span>
              {opt.label}{" "}
              <span className="text-xs text-muted-foreground">({opt.code})</span>
            </span>
            {value === opt.code && <span className="text-info-text" aria-hidden="true">✓</span>}
          </button>
        ))}

        {hasNoResults && (
          <div className="px-3 py-2 text-sm text-muted-foreground">No matches</div>
        )}
      </div>
    </div>
  );
}
