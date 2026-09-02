# PulseTalq design system

> Private productivity at the speed of thought.

## 1. Visual theme and atmosphere

**Style:** Deep Focus  
**Keywords:** restrained, fast, private, layered, exact, calm, desktop-native  
**Tone:** professional and decisive, not clinical, cartoonish, cinematic, or decorative  
**Feel:** a quiet work surface that responds the instant a thought arrives.

**Interaction tier:** L2, fluid interaction  
**Dependencies:** React 18, Framer Motion 11, CSS transitions, and native browser APIs already present in the project. Do not add GSAP, Lenis, WebGL, or a custom cursor.

The app is a working instrument. Visual emphasis follows the user's task: capture, review, connect, continue. The lowercase “PulseTalq” wordmark keeps “pulse” in Blackout and “talq” in Hot Signal. Hot Signal red appears only where speed or an active state matters. Layered surfaces represent accumulated project context, not decoration.

## 2. Color palette and roles

```css
:root {
  /* Backgrounds */
  --pt-bg: #f7f6f2;
  --pt-surface: #ffffff;
  --pt-surface-alt: #efede8;
  --pt-surface-hover: #fff0ec;
  --pt-surface-dark: #18191b;
  --pt-sidebar: #0b0b0c;

  /* Borders */
  --pt-border: #d2cfca;
  --pt-border-strong: #b7b3ad;
  --pt-border-hover: #ff8a73;

  /* Text */
  --pt-text: #0b0b0c;
  --pt-text-secondary: #414042;
  --pt-text-tertiary: #6f6d6d;
  --pt-text-inverse: #f7f6f2;
  --pt-text-inverse-muted: #b9b8b4;

  /* Accent */
  --pt-accent: #ff3b1f;
  --pt-accent-hover: #e92f16;
  --pt-accent-active: #c92510;
  --pt-accent-soft: #ffb39f;
  --pt-accent-wash: #fff0ec;

  /* RGB variants */
  --pt-bg-rgb: 247, 246, 242;
  --pt-surface-rgb: 255, 255, 255;
  --pt-text-rgb: 11, 11, 12;
  --pt-accent-rgb: 255, 59, 31;

  /* Semantic */
  --pt-success: #237a57;
  --pt-success-wash: #e6f3ed;
  --pt-error: #b42318;
  --pt-error-wash: #fbe9e7;
  --pt-warning: #9a5b00;
  --pt-warning-wash: #fff2d6;
  --pt-info: #365f91;
  --pt-info-wash: #e9f0f8;

  /* shadcn compatibility */
  --background: 48 24% 96%;
  --foreground: 240 4% 5%;
  --card: 0 0% 100%;
  --card-foreground: 240 4% 5%;
  --popover: 0 0% 100%;
  --popover-foreground: 240 4% 5%;
  --primary: 240 4% 5%;
  --primary-foreground: 48 24% 96%;
  --secondary: 40 16% 93%;
  --secondary-foreground: 240 4% 5%;
  --muted: 40 16% 93%;
  --muted-foreground: 0 1% 43%;
  --accent: 7 100% 56%;
  --accent-foreground: 240 4% 5%;
  --destructive: 4 75% 40%;
  --destructive-foreground: 0 0% 100%;
  --border: 36 7% 81%;
  --input: 36 7% 81%;
  --ring: 7 100% 56%;
}
```

Color rules:

- Reference colors through variables. Do not hardcode hex values in components.
- Use Readout for the main workspace and Blackout for persistent navigation and primary actions.
- Reserve Hot Signal for recording, the current location, destructive confirmation, and the single primary action in a view.
- Use Afterglow and Accent Wash for selection, transcript relationships, or related project context.
- Never rely on red alone. Pair active and error states with text, an icon, or a shape change.
- Main verified contrast pairs: Blackout on Readout 18.19:1, Readout on Blackout 18.19:1, Blackout on Hot Signal 5.52:1, and body ink on Readout 9.53:1.

## 3. Typography rules

```css
@import url('https://fonts.googleapis.com/css2?family=Archivo:wght@400;500;600&family=Newsreader:opsz,wght@6..72,400;6..72,500&display=swap');

:root {
  --pt-font-ui: "Archivo", "Segoe UI", sans-serif;
  --pt-font-reading: "Newsreader", Georgia, serif;
}
```

| Role | Font | Size | Weight | Line height | Letter spacing |
|---|---|---:|---:|---:|---:|
| App title | Archivo | 40px | 500 | 1.0 | -0.055em |
| View H1 | Archivo | 30px | 500 | 1.1 | -0.04em |
| Section H2 | Archivo | 21px | 500 | 1.2 | -0.025em |
| H3 | Archivo | 16px | 500 | 1.35 | -0.01em |
| UI body | Archivo | 14px | 400 | 1.5 | 0 |
| Reading body | Newsreader | 17px | 400 | 1.55 | 0 |
| Transcript | Newsreader | 16px | 400 | 1.6 | 0 |
| Label | Archivo | 10px | 600 | 1.3 | 0.14em |
| Data | Archivo | 12px | 500 | 1.4 | 0.015em |

Typography rules:

- Use Archivo for controls, navigation, metadata, headings, and compact data.
- Use Newsreader for transcripts, summaries, explanations, and content people read for meaning.
- Body copy is never bold. Use weight 500 for hierarchy and 600 only for small status labels.
- Sentence case is the default. All-caps is limited to labels of 13 characters or fewer.
- Never use Inter, Roboto, Arial, Helvetica, Montserrat, Poppins, or Space Grotesk as the primary face.

Text decoration:

- App H1 and view titles use no gradient and no text shadow. The restrained theme forbids both.
- Section labels may use a 2px Hot Signal rule or an active navigation marker.
- Links use an offset underline on hover. Body text receives no decorative treatment.

## 4. Component stylings

### Buttons

```css
.pt-button {
  min-height: 40px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 0 15px;
  border: 1px solid transparent;
  border-radius: 3px;
  background: var(--pt-text);
  color: var(--pt-text-inverse);
  font: 500 13px/1 var(--pt-font-ui);
  transition: transform 150ms ease, background-color 180ms ease,
    border-color 180ms ease, box-shadow 180ms ease, color 180ms ease;
}
.pt-button:hover {
  transform: translateY(-1px);
  background: var(--pt-surface-dark);
  box-shadow: 0 6px 14px rgba(var(--pt-text-rgb), 0.14);
}
.pt-button:active { transform: translateY(0) scale(0.98); box-shadow: none; }
.pt-button:focus-visible { outline: 2px solid var(--pt-accent); outline-offset: 2px; }
.pt-button:disabled { transform: none; background: var(--pt-border); color: var(--pt-text-tertiary); box-shadow: none; cursor: not-allowed; }
.pt-button--accent { background: var(--pt-accent); color: var(--pt-text); }
.pt-button--accent:hover { background: var(--pt-accent-hover); }
.pt-button--accent:active { background: var(--pt-accent-active); }
.pt-button--secondary { background: var(--pt-surface); color: var(--pt-text); border-color: var(--pt-border-strong); }
.pt-button--secondary:hover { background: var(--pt-surface-hover); border-color: var(--pt-border-hover); }
.pt-button--ghost { background: transparent; color: var(--pt-text-secondary); }
.pt-button--ghost:hover { background: var(--pt-surface-alt); color: var(--pt-text); box-shadow: none; }
```

### Cards and project layers

```css
.pt-card {
  border: 1px solid var(--pt-border);
  border-radius: 3px;
  background: var(--pt-surface);
  color: var(--pt-text);
  box-shadow: 0 1px 2px rgba(var(--pt-text-rgb), 0.025);
  transition: transform 220ms cubic-bezier(.16, 1, .3, 1),
    border-color 180ms ease, box-shadow 220ms ease, background-color 180ms ease;
}
.pt-card:hover {
  transform: translateY(-2px);
  border-color: var(--pt-border-strong);
  box-shadow: 0 12px 28px rgba(var(--pt-text-rgb), 0.08);
}
.pt-card:focus-within {
  border-color: var(--pt-accent);
  box-shadow: 0 0 0 3px rgba(var(--pt-accent-rgb), 0.10);
}
.pt-card[aria-selected="true"] { border-left: 2px solid var(--pt-accent); background: var(--pt-accent-wash); }
.pt-layer-stack > *:nth-child(2) { transform: translateX(10px); background: var(--pt-accent-wash); }
.pt-layer-stack > *:nth-child(3) { transform: translateX(20px); background: var(--pt-surface-dark); color: var(--pt-text-inverse); }
```

### Navigation

```css
.pt-sidebar { background: var(--pt-sidebar); color: var(--pt-text-inverse); }
.pt-nav-item {
  position: relative;
  min-height: 44px;
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 0 14px;
  border-radius: 3px;
  color: var(--pt-text-inverse-muted);
  transition: color 180ms ease, background-color 180ms ease;
}
.pt-nav-item:hover { color: var(--pt-text-inverse); background: rgba(var(--pt-surface-rgb), 0.07); }
.pt-nav-item:active { background: rgba(var(--pt-surface-rgb), 0.11); }
.pt-nav-item:focus-visible { outline: 2px solid var(--pt-accent); outline-offset: -2px; }
.pt-nav-item[aria-current="page"] { color: var(--pt-text-inverse); background: rgba(var(--pt-surface-rgb), 0.09); }
.pt-nav-item[aria-current="page"]::before {
  content: "";
  position: absolute;
  inset-block: 10px;
  left: 0;
  width: 2px;
  background: var(--pt-accent);
}
```

### Links

```css
.pt-link { color: inherit; text-decoration: none; background-image: linear-gradient(var(--pt-accent), var(--pt-accent)); background-position: 0 100%; background-repeat: no-repeat; background-size: 0 1px; transition: color 180ms ease, background-size 220ms ease; }
.pt-link:hover { color: var(--pt-text); background-size: 100% 1px; }
.pt-link:active { color: var(--pt-accent-active); }
.pt-link:focus-visible { outline: 2px solid var(--pt-accent); outline-offset: 3px; }
.pt-link[aria-disabled="true"] { color: var(--pt-text-tertiary); pointer-events: none; }
```

### Tags and badges

```css
.pt-badge { display: inline-flex; align-items: center; min-height: 24px; padding: 0 8px; border: 1px solid var(--pt-border); border-radius: 2px; background: var(--pt-surface); color: var(--pt-text-secondary); font: 600 10px/1 var(--pt-font-ui); letter-spacing: .04em; }
.pt-badge--live { border-color: var(--pt-border-hover); background: var(--pt-accent-wash); color: var(--pt-text); }
.pt-badge--success { border-color: var(--pt-success); background: var(--pt-success-wash); color: var(--pt-success); }
.pt-badge--warning { border-color: var(--pt-warning); background: var(--pt-warning-wash); color: var(--pt-warning); }
```

### Form controls

```css
.pt-input { min-height: 42px; width: 100%; border: 1px solid var(--pt-border-strong); border-radius: 3px; background: var(--pt-surface); color: var(--pt-text); padding: 0 13px; font: 400 14px/1 var(--pt-font-ui); transition: border-color 180ms ease, box-shadow 180ms ease, background-color 180ms ease; }
.pt-input:hover { border-color: var(--pt-text-tertiary); }
.pt-input:focus { outline: none; border-color: var(--pt-accent); box-shadow: 0 0 0 3px rgba(var(--pt-accent-rgb), .10); }
.pt-input:disabled { background: var(--pt-surface-alt); color: var(--pt-text-tertiary); cursor: not-allowed; }
.pt-input[aria-invalid="true"] { border-color: var(--pt-error); box-shadow: 0 0 0 3px rgba(180, 35, 24, .09); }
```

### Recording command bar

The recording bar is a compact horizontal instrument panel, not a rounded floating pill. Use a 3px radius, dark surface, status copy, device affordance, timer, and one Hot Signal control. The recording state adds a 2px Hot Signal top edge. Waveform bars use Hot Signal and Afterglow with transform-only animation.

## 5. Layout principles

**Application frame:**

- Full window: `100dvh`, overflow hidden at the shell.
- Sidebar expanded: 256px. Collapsed: 64px.
- Optional context rail: 320px to 360px.
- Main workspace: fluid, minimum 0, with its own vertical scroll container.
- Content max width inside a view: 1180px.
- Workspace padding: 32px desktop, 24px compact, 18px mobile.

**Spacing scale:** 4, 8, 12, 16, 24, 32, 48, 64px.  
**Component gap:** 12 to 16px.  
**Card padding:** 16px compact, 20px default, 24px feature.  
**Section gap:** 32px within a screen. Avoid marketing-page spacing inside the app.

```css
.pt-app-grid { display: grid; grid-template-columns: var(--sidebar-width, 256px) minmax(0, 1fr); height: 100dvh; }
.pt-workspace { min-width: 0; overflow: auto; padding: 32px; background: var(--pt-bg); }
.pt-content-grid { display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 16px; max-width: 1180px; margin: 0 auto; }
.pt-main-panel { grid-column: span 8; }
.pt-context-panel { grid-column: span 4; }
```

## 6. Depth and elevation

| Level | Treatment | Use |
|---|---|---|
| Flat | no shadow, 1px border | sidebar items, tables, passive transcript rows |
| Subtle | `0 1px 2px rgba(11,11,12,.025)` | default cards and inputs |
| Raised | `0 8px 20px rgba(11,11,12,.07)` | menus, selected cards, command bar |
| Elevated | `0 14px 34px rgba(11,11,12,.10)` | modals, active project context, onboarding |
| Focus | `0 0 0 3px rgba(255,59,31,.10)` | keyboard and field focus only |

Depth rules:

- Borders establish structure before shadows do.
- Only one elevation level may dominate a view.
- Context layers may overlap by 8 to 12px. They may not rotate or use perspective tilt.
- Do not use glassmorphism or large blurred backdrops.

## 7. Animation and interaction

**Motion philosophy:** fast enough to preserve flow, slow enough to explain state. Use opacity and transform.  
**Tier:** L2  
**Dependencies:** Framer Motion already installed. CSS handles continuous waveform and simple control transitions.

Six required motion categories:

1. App/view title: staggered word or line reveal using Framer Motion variants.
2. Section headings: 8px fade-and-slide when a panel enters.
3. Body and labels: short stagger on first view load, never character-by-character.
4. Element feedback: buttons press to 0.98 scale; cards lift 2px.
5. Interactive component: project-context layers settle into position and active navigation marker moves with `layoutId`.
6. Atmospheric layer: a low-contrast static signal-line field in empty and onboarding states. During recording only, the signal lines animate with transforms.

```tsx
export const viewMotion = {
  hidden: { opacity: 0, y: 10 },
  show: { opacity: 1, y: 0, transition: { duration: 0.28, ease: [0.16, 1, 0.3, 1], staggerChildren: 0.045 } },
};

export const itemMotion = {
  hidden: { opacity: 0, y: 8 },
  show: { opacity: 1, y: 0, transition: { duration: 0.24, ease: [0.16, 1, 0.3, 1] } },
};
```

```css
@keyframes pt-wave {
  0%, 100% { transform: scaleY(.32); opacity: .58; }
  50% { transform: scaleY(1); opacity: 1; }
}
.pt-wave-bar { transform-origin: center; animation: pt-wave 720ms ease-in-out infinite; }
.pt-wave-bar:nth-child(2n) { animation-duration: 880ms; animation-delay: -180ms; }
.pt-wave-bar:nth-child(3n) { animation-duration: 640ms; animation-delay: -320ms; }
```

Route and panel transitions use `AnimatePresence` with 180ms exits and 280ms entrances. Avoid exit motion on destructive or time-sensitive actions. Navigation uses a shared `layoutId="active-nav"` marker. Menus scale from 0.98 to 1 while fading. Toasts slide 8px, not across the screen.

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: .01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: .01ms !important;
    scroll-behavior: auto !important;
  }
  .pt-wave-bar { transform: scaleY(.58); }
}
```

## 8. Do's and don'ts

### Do

- Put the current task first and show one clear primary action per view.
- Use Hot Signal to show capture, active location, or a decision that needs attention.
- Use Newsreader where users review words for meaning.
- Keep privacy claims concrete: “Audio stays on this device.”
- Let project context appear as ordered layers with clear labels and timestamps.
- Preserve keyboard navigation, visible focus, and semantic control labels.
- Use Lucide icons already installed, at 16 or 18px with 1.5 to 2px strokes.
- Keep empty states actionable and specific.

### Don't

- Do not use gradients, gradient text, or gradient buttons.
- Do not use glassmorphism, frosted sidebars, or large backdrop blur.
- Do not put every surface in a rounded card. Radius stays at 3px.
- Do not use decorative red backgrounds or red for passive information.
- Do not use purple, violet, or blue as extra brand accents.
- Do not use icon-in-circle feature grids.
- Do not use rotating, tilted, bubbly, or toy-like geometry.
- Do not use generic copy such as “Unleash productivity” or “Built for creators.”
- Do not add emoji as interface icons.
- Do not animate transcripts while a user is selecting or editing text.
- Do not use parallax, scroll-jacking, custom cursors, or cinematic transitions in the app.
- Do not add a new motion library while Framer Motion covers the requirement.

## 9. Responsive behavior

| Name | Width | Key changes |
|---|---:|---|
| Wide desktop | > 1200px | 256px sidebar, main workspace, optional 340px context rail |
| Compact desktop | 760px to 1200px | 220px sidebar or 64px collapsed, context panel becomes a drawer below 1000px |
| Mobile/narrow window | < 760px | 64px icon rail or overlay navigation, single-column workspace, bottom command bar spans available width |
| Small mobile | < 600px | 18px workspace padding, stacked controls, abbreviated metadata, no layered offsets beyond 6px |

**Touch targets:** 44px minimum for navigation, recording, menus, and destructive actions.  
**Collapsing strategy:** context rail becomes an accessible sheet, two-column panels stack, tables switch to labeled rows, and nonessential metadata hides before actions do.

```css
@media (max-width: 1200px) {
  .pt-workspace { padding: 24px; }
  .pt-content-grid { grid-template-columns: repeat(8, minmax(0, 1fr)); }
  .pt-main-panel { grid-column: 1 / -1; }
  .pt-context-panel { display: none; }
}

@media (max-width: 760px) {
  .pt-app-grid { grid-template-columns: 64px minmax(0, 1fr); }
  .pt-workspace { padding: 20px; }
  .pt-content-grid { display: block; }
  .pt-button, .pt-nav-item { min-height: 44px; }
}

@media (max-width: 600px) {
  .pt-workspace { padding: 18px; }
  .pt-button-row { display: grid; grid-template-columns: 1fr; }
  .pt-layer-stack > * { transform: none !important; }
}
```
