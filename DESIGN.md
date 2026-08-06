# Design

Visual system for Conversationaly. Every value here exists as a CSS custom
property in `frontend/src/app/globals.css` and, where useful, as a Tailwind
token in `frontend/tailwind.config.js`. **`globals.css` is the source of truth.**
No component may hardcode a Tailwind palette color (`bg-gray-50`, `text-blue-600`,
`bg-red-500`). If a color is needed that isn't here, add it here first.

## Theme

Two first-class themes. Default follows `prefers-color-scheme`; a manual override
persists to `localStorage` under `conversationaly.theme` and is applied by an
inline script in `<head>` before paint, so there is no flash.

Dark is the primary *working* theme (monitor-lit room, 90-minute session beside a
call). Light is the primary *reading* theme (bright room, reviewing a summary).
Neither is a downgrade of the other — they have independent token values, not a
filter.

## Color

OKLCH throughout. Hue anchors: **brand 110°** (olive), **danger 25°** (red),
**warn 72°** (amber), **info 262°** (indigo).

### Strategy: Restrained

Tinted neutrals carry ~92% of every surface. One brand color for identity,
primary action, and current selection. Red is not part of the palette — it is a
**signal**, reserved for live capture and destructive actions, and it appears
nowhere else. Success does not get its own hue: the brand olive *is* the success
color, so "working correctly" and "this product" read as the same thing.

Neutrals are tinted 0.004–0.014 chroma toward 110°. This is below the threshold
of "warm-tinted" — it keeps grays from reading as dead digital gray without
landing anywhere near cream.

### Light

| Role | Value | Use |
|---|---|---|
| `--bg` | `oklch(1 0 0)` | Canvas. Pure white — the transcript and summary are documents. |
| `--panel` | `oklch(0.976 0.004 110)` | Sidebar, toolbars, rails. The second neutral layer. |
| `--elevated` | `oklch(1 0 0)` | Popovers, dialogs, menus. Sits on a scrim with a border + shadow. |
| `--sunken` | `oklch(0.968 0.005 110)` | Input wells, code, inset readouts. |
| `--border` | `oklch(0.912 0.006 110)` | Default hairline. |
| `--border-strong` | `oklch(0.855 0.008 110)` | Input outlines, dividers that must read. |
| `--ink` | `oklch(0.215 0.013 110)` | Body text. **17.5:1** on `--bg`. |
| `--ink-muted` | `oklch(0.46 0.014 110)` | Secondary text. **7.1:1** — deliberately darker than the usual muted gray. |
| `--ink-faint` | `oklch(0.54 0.012 110)` | Tertiary / metadata. **5.1:1**. |
| `--brand` | `oklch(0.365 0.082 110)` | Primary buttons, active nav, success. White text: 11.3:1. |
| `--brand-hover` | `oklch(0.315 0.078 110)` | |
| `--brand-soft` | `oklch(0.955 0.022 110)` | Selection / active-row tint. |
| `--brand-soft-ink` | `oklch(0.33 0.075 110)` | Text on `--brand-soft`. 10.7:1. |
| `--danger` | `oklch(0.545 0.205 25)` | Record button, destructive fills. White text: 4.95:1. |
| `--danger-ink` | `oklch(0.47 0.19 25)` | Red text on canvas. 6.8:1. |
| `--danger-soft` | `oklch(0.962 0.022 25)` | Destructive-state backgrounds. |
| `--warn` / `--warn-ink` / `--warn-soft` | `oklch(0.72 0.15 72)` / `oklch(0.47 0.11 72)` / `oklch(0.966 0.03 72)` | Permission gaps, degraded state. |
| `--info` / `--info-ink` / `--info-soft` | `oklch(0.52 0.115 262)` / `oklch(0.52 0.115 262)` / `oklch(0.962 0.018 262)` | Local-model and device readouts. 5.5:1. |

### Dark

| Role | Value | Use |
|---|---|---|
| `--bg` | `oklch(0.155 0.004 110)` | Canvas. |
| `--panel` | `oklch(0.185 0.005 110)` | Sidebar, toolbars. |
| `--elevated` | `oklch(0.225 0.006 110)` | Popovers, dialogs. |
| `--sunken` | `oklch(0.125 0.004 110)` | Input wells. |
| `--border` | `oklch(0.285 0.007 110)` | |
| `--border-strong` | `oklch(0.37 0.009 110)` | |
| `--ink` | `oklch(0.945 0.006 110)` | **16.7:1**. |
| `--ink-muted` | `oklch(0.72 0.011 110)` | **7.9:1**. |
| `--ink-faint` | `oklch(0.625 0.01 110)` | **5.0:1**. |

**All three ink tiers clear 4.5:1 against all four surfaces (`bg`, `panel`,
`sunken`, `elevated`) in both themes** — verified by measuring the rendered
values in the browser, not by eye. There is deliberately no "large text only"
tier: in a codebase this size that becomes a footgun the moment someone reaches
for the lightest gray on a caption.
| `--brand` | `oklch(0.8 0.115 110)` | Bright sage. **Takes dark ink, not white** — `--brand-ink` `oklch(0.16 0.03 110)`, 10.4:1. |
| `--brand-soft` | `oklch(0.255 0.032 110)` | |
| `--danger` | `oklch(0.55 0.21 25)` | Held at L 0.55 so white text still passes (4.9:1). |
| `--danger-ink` | `oklch(0.72 0.16 25)` | 7.9:1. |

The brand flips polarity between themes: a deep olive that carries white text in
light, a bright sage that carries dark text in dark. This is intentional — it is
what keeps the accent legible without either theme feeling like a tint of the
other.

## Typography

**One superfamily: IBM Plex.** Three optical registers, shared metrics, designed
together — not a pairing of two similar sans.

- **Plex Sans** (`--font-sans`) — all UI chrome, labels, buttons, navigation,
  transcript body. Speech is not prose; sans is the honest setting for it.
- **Plex Serif** (`--font-serif`) — generated summary body and large meeting
  titles only. The review surface is a document and should read like one.
- **Plex Mono** (`--font-mono`) — timestamps, durations, model IDs, device names,
  confidence values, file paths, version. Anything that is a *machine fact*.
  This is principle 3 made visible: the local machinery is set in a typeface
  that says "readout".

Fixed rem scale, 16px root, ratio ~1.08 at UI sizes and ~1.2 above. No `clamp()` —
users view at a consistent DPI and a fluid heading in a 256px sidebar looks worse.

| Token | Size / line-height | Use |
|---|---|---|
| `text-2xs` | 11 / 15 | Mono readouts, timestamps |
| `text-xs` | 12 / 17 | Captions, meta |
| `text-sm` | 13 / 19 | Dense labels, buttons |
| `text-base` | 14 / 21 | Default UI body |
| `text-md` | 15 / 24 | Transcript body |
| `text-lg` | 17 / 27 | Summary body (serif) |
| `text-xl` | 20 / 27 | Panel titles |
| `text-2xl` | 25 / 31 | Page titles |
| `text-3xl` | 31 / 37 | Meeting title |

Prose measure capped at 68ch (`--measure`). Transcript and summary both respect
it. `text-wrap: balance` on titles, `pretty` on prose.

## Shape & elevation

Radii are tight — instrument, not app-store icon.

`--r-sm 4px` · `--r-md 6px` · `--r-lg 10px` · `--r-xl 14px` · `--r-full 999px`

Elevation is border-first: a hairline always, a shadow only when the element
genuinely floats (popover, dialog, the recording transport). Two shadow tokens,
`--shadow-pop` and `--shadow-float`. No shadow on static cards.

## Layout

- Sidebar rail: 256px expanded / 56px collapsed, `--panel`, hairline right border.
- Content max measure 68ch for prose; toolbars and tables run full width.
- Responsive behavior is **structural** (collapse the rail, stack the two-pane
  meeting view below 1100px), never fluid type.
- The recording transport is `position: fixed`, bottom-centered on the content
  column, and offsets with the rail via a CSS variable, not inline style math.

## Motion

Tokens: `--dur-fast 120ms` · `--dur 180ms` · `--dur-slow 260ms`,
easing `--ease` `cubic-bezier(0.16, 1, 0.3, 1)`.

Motion reports state and nothing else — a level changing, a status advancing, a
segment arriving, a panel collapsing. There is no page-load choreography: the
current `motion.div` fade-and-rise on every route mount is removed. It makes a
tool the user opens forty times a day feel slow.

`prefers-reduced-motion: reduce` → all durations collapse to 1ms except opacity
crossfades, and the audio level meter stops animating and renders a static
numeric readout instead. Reduced motion must not remove information.

## Z-index

Semantic scale only. No arbitrary values.

`--z-dropdown 100` · `--z-sticky 200` · `--z-rail 300` · `--z-overlay 400` ·
`--z-modal 500` · `--z-toast 600` · `--z-tooltip 700`

## Component rules

- Every interactive element ships all seven states: default, hover, focus-visible,
  active, disabled, loading, error. Half a set is a bug.
- One button vocabulary across every screen: `primary` (brand fill), `secondary`
  (border + `--panel`), `ghost` (transparent, hover tint), `danger` (red fill).
  Sizes `sm` / `md`. Nothing else.
- Loading is a skeleton in content areas; a spinner only inside a button or on a
  control smaller than 32px.
- Empty states teach the next action and name it as a button.
- Focus ring: `2px` `--ring` with a `2px` `--bg` offset, on `:focus-visible` only.
- Recording state is never communicated by color alone — the live indicator is a
  filled dot **plus** the word "Recording" **plus** an elapsed mono timer.
