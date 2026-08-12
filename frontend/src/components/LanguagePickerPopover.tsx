"use client";

import { useMemo, useState } from "react";
import { LANGUAGE_OPTIONS } from "@/lib/summary-languages";
import { useRecentLanguages } from "@/hooks/useRecentLanguages";
import { useT } from "@/lib/i18n";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import { MaterialSymbol } from "@/vendor/deslop/primitives/material-symbols-react";
import { cn } from "@/lib/utils";

interface LanguagePickerPopoverProps {
  value: string | null;
  onChange: (code: string | null) => void;
  mode?: "meeting" | "settings";
  autoSubtitle?: string;
  className?: string;
}

export function LanguagePickerPopover({
  value,
  onChange,
  mode = "meeting",
  autoSubtitle,
  className,
}: LanguagePickerPopoverProps) {
  const t = useT();
  const { recents } = useRecentLanguages();
  const [query, setQuery] = useState("");

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

  const autoLabel = t('Auto');
  const showAuto = mode === "meeting" && (
    !filter || "auto".includes(filter) || autoLabel.toLowerCase().includes(filter)
  );
  const showRecents = mode === "meeting" && recentsResolved.length > 0;
  const hasNoResults =
    filteredAll.length === 0 && recentsResolved.length === 0 && !showAuto;

  return (
    <Command
      shouldFilter={false}
      className={cn(
        "w-72 rounded-lg border border-border bg-background text-foreground shadow-none",
        className,
      )}
      role="dialog"
      aria-label={t('Pick summary language')}
    >
      <CommandInput
        autoFocus
        value={query}
        onValueChange={setQuery}
        placeholder={t('Search language...')}
        className="text-sm"
      />

      <CommandList className="max-h-80">
        {hasNoResults && <CommandEmpty>{t('No matches')}</CommandEmpty>}

        {showRecents && (
          <>
            <CommandGroup heading={t('Recently Used')}>
              {recentsResolved.map((opt) => (
                <LanguageCommandItem
                  key={`recent-${opt.code}`}
                  code={opt.code}
                  label={opt.label}
                  selected={value === opt.code}
                  onSelect={() => onChange(opt.code)}
                />
              ))}
            </CommandGroup>
            <CommandSeparator />
          </>
        )}

        {showAuto && (
          <CommandItem
            value="auto"
            aria-pressed={value === null}
            onSelect={() => onChange(null)}
            className="min-h-10 px-3 py-2"
          >
            <span className="flex min-w-0 flex-1 flex-col">
              <span>{autoLabel}</span>
              {autoSubtitle && (
                <span className="text-xs font-normal text-muted-foreground">{autoSubtitle}</span>
              )}
            </span>
            <SelectionCheck selected={value === null} />
          </CommandItem>
        )}

        {filteredAll.length > 0 && (
          <CommandGroup heading={mode === "meeting" ? t("Other Languages") : t("All Languages")}>
            {filteredAll.map((opt) => (
              <LanguageCommandItem
                key={`all-${opt.code}`}
                code={opt.code}
                label={opt.label}
                selected={value === opt.code}
                onSelect={() => onChange(opt.code)}
              />
            ))}
          </CommandGroup>
        )}
      </CommandList>
    </Command>
  );
}

function SelectionCheck({ selected }: { selected: boolean }) {
  return (
    <MaterialSymbol
      name="check"
      size={16}
      weight={600}
      aria-hidden="true"
      className={cn("ml-auto shrink-0", selected ? "opacity-100" : "opacity-0")}
    />
  );
}

function LanguageCommandItem({
  code,
  label,
  selected,
  onSelect,
}: {
  code: string;
  label: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <CommandItem
      value={`${label} ${code}`}
      aria-pressed={selected}
      onSelect={onSelect}
      className="min-h-9 px-3"
    >
      <span className="min-w-0 flex-1 truncate">
        {label}{" "}
        <span className="text-xs text-muted-foreground">({code})</span>
      </span>
      <SelectionCheck selected={selected} />
    </CommandItem>
  );
}
