# React Bits components

Copied unchanged from apps/landing/components/rb (itself from [React Bits](https://reactbits.dev),
TypeScript + Tailwind variant, fetched 2026-11-02). Licence: MIT + Commons Clause, see `LICENSE.md`:
use in our product is allowed; selling or redistributing the components themselves is not.

The overview imports its components through `island` from @genesy/ui, so the code is fetched lazily
and only when the user allows motion. The only change is a first line `// @ts-nocheck` (assistant-screen files: see below). Do not edit
them otherwise; wrap them instead, so a later copy can replace them.

`MagicBento.tsx` (fetched 2026-11-08) also swaps `@hugeicons` for a lucide-react shim; see
`MagicBento-notice.md`. The overview renders it on wide screens only, with the plain grid as fallback.

## Assistant screen (components/voice)

`ThoughtLine`, `AnimatedList` and `PromptBar` are the assistant's working parts (thinking line,
review rows, text composer), not decoration, so the voice screen imports them directly instead of
through `island`; each honours reduced motion itself. Source, date and edits per file:
`ThoughtLine-notice.md`, `AnimatedList-notice.md`, `PromptBar-notice.md`. `ThoughtLine` (with its
notice and a copy of `LICENSE.md`) now lives in `packages/voice/src/components/rb`, next to the
shared assistant engine that uses it. PromptBar carries a few
more edits than the others (translated labels, hint, `ready`, keep the draft on a refused send,
"+" menu focus order), marked
`// Genesy edit:` in the file. Their palettes are overridden in `app/office.css` under the voice
comments.
