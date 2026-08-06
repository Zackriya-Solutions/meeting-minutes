# Product

## Register

product

## Users

People who sit in back-to-back meetings and need a record of them, working on
their own machine. The defining context is **peripheral**: Conversationaly runs
in a narrow window beside a video call for 30–120 minutes while the user's
attention is on the call, not on us. They glance over every few minutes to
confirm one thing — *is it still capturing?* — and look properly only twice: at
the start (pick devices, confirm the model is loaded) and after (read the
summary, fix the title, copy it somewhere).

A second, distinct context: reviewing a past meeting the next morning. Here the
app is a reading surface, not an instrument. Long transcript, generated summary,
editing and copying.

The job: **capture a conversation reliably, without sending it anywhere, and
hand back something readable.**

## Product Purpose

A privacy-first meeting assistant that records, transcribes, and summarizes
entirely on local hardware — no cloud transcription, no account, no upload.
Transcription runs through transcribe.cpp locally; summaries run through a
bundled local LLM sidecar or a user-configured provider.

Success is invisibility during the meeting and confidence after it. A user
should never wonder whether it is still recording, and never wonder where their
audio went.

## Brand Personality

**Calm, precise, local.**

Voice is a well-made instrument: it states, it does not sell. No exclamation
marks, no "Awesome!", no emoji in product chrome. Labels name what a thing does
("Start recording", "Model not downloaded"), never what we hope the user feels.
Errors say what happened and what to do next, in that order.

The interface should feel like professional capture equipment that happens to
render documents: matte, quiet, one indicator lamp. It gets out of the way while
running, and reads like a well-set page afterwards.

## Anti-references

- **The generic shadcn dashboard** — gray card grids, blue-600 accents,
  Inter everywhere, everything a rounded box on `bg-gray-50`. This is what the
  interface looks like today and it is the thing being replaced.
- **Terminal cosplay** — monospace-everything, phosphor green on pure black,
  scanlines. "Local-first" does not mean "1983".
- **Warm-paper editorial** — cream/sand/parchment backgrounds with a big serif.
  Document-first is about typographic care, not a beige surface.
- **Consumer AI sparkle** — gradient text, glowing purple orbs, animated
  "thinking" shimmer, a mascot. The current cartoon-pencil logo goes.
- **Alarm-state chrome** — red as a decorative accent. Red means one thing here.

## Design Principles

1. **Recording is the only loud thing.** Red is reserved, absolutely, for live
   capture and destructive actions. Nothing else on the surface competes with
   it. When nothing is recording, the interface is monochrome plus one quiet
   brand color.

2. **The transcript is the interface.** Chrome shrinks as content arrives.
   Toolbars, panels, and labels earn their pixels against the words on screen.

3. **Make the local machine legible.** Which model is loaded, which devices are
   captured, where the file lives, how confident the decode was — surfaced as
   readable facts, not hidden behind a settings modal. Privacy is proven by
   showing the machinery, not by claiming it.

4. **Two registers, one voice.** Live capture is an instrument (dense, mono
   readouts, status-first). Review is a document (generous measure, editorial
   type). Same tokens, same components, different density.

5. **Nothing moves that isn't reporting state.** Motion means something changed:
   a level, a status, a new segment. No entrance choreography, no page-load
   sequences, no decorative animation.

## Accessibility & Inclusion

- **WCAG 2.2 AA minimum**, both themes. Body text ≥ 4.5:1; the secondary/muted
  text ramp is tuned to ≥ 5:1, not the usual light-gray default.
- **Never color alone.** Recording state, confidence level, and connection
  status each carry a shape, icon, or text label in addition to color — the
  live indicator is a filled dot *plus* the word, not a red dot alone.
- Visible `:focus-visible` ring on every interactive element, with a contrast
  offset that survives both themes.
- Full keyboard reach for the recording transport (start / pause / stop) and
  sidebar navigation. Recording must never be startable or stoppable by mouse
  only.
- `prefers-reduced-motion: reduce` collapses the audio level meter to a static
  state readout and removes all transitions except opacity.
- `prefers-contrast: more` supported via a higher-contrast border and ink ramp.
- Theme respects `prefers-color-scheme` by default, with a persisted manual
  override.
