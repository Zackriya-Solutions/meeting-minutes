# Deslop primitives snapshot

This directory contains the font and icon primitives used by Memento.

- Source: `mishanaer/deslop/primitives`
- Typography: SB Sans Interface and SB Sans Text Mono
- Icons: Material Symbols Rounded through Deslop's generated React renderer

The files are vendored because the source repository does not currently expose
`primitives` as an installable package. Keep this directory byte-for-byte in
sync with the source assets; product-specific icon aliases live outside this
directory in `src/components/deslop-icons.tsx`.
