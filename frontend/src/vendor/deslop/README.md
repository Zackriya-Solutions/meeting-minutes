# Deslop primitives snapshot

This directory contains the font, icon, and surface color primitives used by Memento.

- Source: `mishanaer/deslop/primitives`
- Typography: SB Sans Interface, SB Serif Text, and SB Sans Text Mono
- Icons: Material Symbols Rounded through Deslop's generated React renderer
- Colors: `--elevation-1`, `--primary-5`, `--primary-10`, and `--primary-40`,
  mirrored from `primitives/colors.css` for light and dark themes

The files are vendored because the source repository does not currently expose
`primitives` as an installable package. Keep asset bytes and token values in
sync with the source; Memento adapts Deslop's color-scheme selector to the
`.dark` class used by `next-themes`. Product-specific icon aliases live outside
this directory in `src/components/deslop-icons.tsx`.

`mini-app/Cell.tsx` is the desktop TypeScript port of Deslop's Mini App `Cells`
component. It keeps the source spacing, separators, typography, and interaction
states while using Memento's existing React runtime. Deslop's `--primary*`
colors are exposed as `--deslop-primary*` here because shadcn already owns the
unprefixed `--primary` name in Memento.
