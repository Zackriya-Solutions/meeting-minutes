# Recording content window

## Problem

Imported and long-running recordings can contain a real meeting followed by a long silent gap and a handful of accidental microphone fragments. Feeding those fragments into a summary can create false decisions, tasks, names, or safety issues.

## Implemented boundary

The app performs a local deterministic check over timestamped transcript segments. It only suggests a primary meeting window when all of these conditions hold:

- at least five primary segments and 400 characters precede the split;
- the first silent gap is at least ten minutes;
- no more than twelve transcript fragments follow it;
- those fragments are at most 300 characters and at most 10% of the primary transcript.

This is deliberately conservative. A substantial second content window remains part of the meeting.

The suggestion never deletes or edits audio or transcript rows. It is only applied to future summary and regeneration input after an explicit user choice, can be reset to the full transcript, and safely falls back to the full transcript on errors.

## Evidence and follow-up

The need was found while reviewing the imported corpus: a product/release sync had its primary conversation in roughly the first twenty minutes, followed by a gap longer than twenty minutes and a few isolated late fragments.

A check over all 50 imported recordings produced three suggestions. Manual transcript review confirmed that all three excluded only sparse post-meeting/open-microphone fragments. A separate multi-session recording correctly produced no suggestion because its later conversation exceeded the sparse-tail limits. This is encouraging precision evidence, not a final benchmark; future held-out recordings are still required before thresholds are relaxed.

Useful next steps:

1. measure suggestion precision on the reviewed corpus and record false positives;
2. support a manually adjustable start/end window when deterministic detection is unavailable;
3. expose the selected window to standup evaluation exports so gold annotations describe the same evidence;
4. consider multiple user-confirmed sessions inside one recording, never automatic destructive splitting.
