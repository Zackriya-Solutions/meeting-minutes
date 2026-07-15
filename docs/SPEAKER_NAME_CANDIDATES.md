# Safe speaker name and alias candidates

## Product boundary

Transcript text is untrusted. A heard name is a candidate, never an identity update. The
feature runs locally and requires the user to inspect evidence, select a diarized speaker,
and separately opt in before changing that speaker's display name.

## Evidence levels

- self-introduction (`Меня зовут …` / `My name is …`) may suggest the current diarized
  speaker with high confidence;
- explicit introduction (`Это …`, `С нами …`) produces an unlinked candidate because the
  introduced person may not be the speaker;
- direct address is linked to the next responding speaker only when it occurs within 15
  seconds, and is hidden until the same name-to-speaker relation occurs at least twice in
  the meeting.

Scanning is idempotent: reopening the dialog does not turn one observation into two.
Candidates keep a timestamp and quote for review. No LLM or network request is used.

## Safety and retention

- reject control characters, digits, implausible shapes, roles, generic address words,
  profanity, and insults before candidate storage;
- never log rejected raw values;
- persist rejected unsafe values only as SHA-256 over an installation-local random salt and
  normalized candidate, plus a non-content reason code;
- keep the salt in a secret-classified app setting so it never crosses the Tauri IPC settings
  response;
- when a user rejects a previously safe candidate, clear its candidate text and evidence
  quote and retain only the salted fingerprint and status.

## Aliases and identity

Confirmed aliases belong to a speaker profile and do not overwrite a confirmed display name
unless the user checks “Use as display name”. The same first name may belong to multiple
speaker profiles; an alias alone is never enough for a cross-meeting merge. Future merge
suggestions must combine repeated linguistic evidence with compatible voice evidence and
still require confirmation. Nicknames and transliterations may be attached as separate
aliases to the same profile, but are never generalized automatically from a nickname
dictionary.
