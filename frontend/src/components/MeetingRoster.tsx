"use client";

import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { MaterialSymbol } from "@/vendor/deslop/primitives/material-symbols-react";
import { useT } from "@/lib/i18n";
import {
  readMeetingRoster,
  writeMeetingRoster,
} from "@/lib/meetingParticipants";

/**
 * Who is in the room, typed while the recording runs.
 *
 * This does not attribute anything live and cannot: diarization runs after the meeting, so
 * during a recording there are no voices to attach a name to. What it produces is a closed
 * list of names, which is exactly what the naming pass lacks afterwards — it reads the
 * meeting's participants as hints, and a list of three turns "guess who this voice is" into
 * "match these three names to these three voices". Until now that list could only come from
 * an Outlook invitation.
 *
 * It also covers the case a transcript can never solve on its own: someone who is named out
 * loud but never says their own name, and who therefore ends up missing from the summary
 * while being discussed in it.
 *
 * The names live in session storage until the meeting row exists; the stop sequence attaches
 * them (see `useRecordingStop`).
 */
export function MeetingRoster() {
  const t = useT();
  const [names, setNames] = useState<string[]>([]);
  const [draft, setDraft] = useState("");
  const [adding, setAdding] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Session storage is not available during the server render.
  useEffect(() => setNames(readMeetingRoster(sessionStorage)), []);

  const commit = (next: string[]) => {
    setNames(next);
    writeMeetingRoster(sessionStorage, next);
  };

  const add = () => {
    const name = draft.trim();
    setDraft("");
    if (!name) {
      setAdding(false);
      return;
    }
    if (!names.some((existing) => existing.toLocaleLowerCase() === name.toLocaleLowerCase())) {
      commit([...names, name]);
    }
    inputRef.current?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      add();
      return;
    }
    if (event.key === "Escape") {
      // The recording panel is a route drawer that closes on Escape. Leaving the field is
      // what the key means here; leaving the meeting is not.
      event.stopPropagation();
      event.preventDefault();
      setDraft("");
      setAdding(false);
      return;
    }
    if (event.key === "Backspace" && draft === "" && names.length > 0) {
      commit(names.slice(0, -1));
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="text-xs text-[var(--deslop-primary-50)]">{t("Who is here")}</span>

      {names.map((name) => (
        <span
          key={name.toLocaleLowerCase()}
          className="inline-flex items-center gap-1 rounded-full bg-[var(--primary-5)] py-1 pl-2.5 pr-1 text-xs text-[var(--deslop-primary)]"
        >
          {name}
          <button
            type="button"
            aria-label={`${t("Remove")} ${name}`}
            className="grid size-4 place-items-center rounded-full text-[var(--deslop-primary-50)] hover:text-[var(--deslop-primary)]"
            onClick={() => commit(names.filter((existing) => existing !== name))}
          >
            <MaterialSymbol name="close" size={12} weight={400} />
          </button>
        </span>
      ))}

      {adding ? (
        <input
          ref={inputRef}
          autoFocus
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={handleKeyDown}
          onBlur={add}
          placeholder={t("Name")}
          aria-label={t("Participant name")}
          className="w-28 rounded-full bg-[var(--primary-5)] px-2.5 py-1 text-xs text-[var(--deslop-primary)] outline-none placeholder:text-[var(--deslop-primary-40)]"
        />
      ) : (
        <button
          type="button"
          onClick={() => setAdding(true)}
          className="inline-flex items-center gap-1 rounded-full px-2 py-1 text-xs text-[var(--deslop-primary-50)] hover:text-[var(--deslop-primary)]"
        >
          <MaterialSymbol name="add" size={14} weight={400} />
          {names.length === 0 ? t("Add participants") : t("Add")}
        </button>
      )}
    </div>
  );
}
