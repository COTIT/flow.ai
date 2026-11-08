# MagicBento.tsx

- Source: [React Bits — Magic Bento](https://reactbits.dev/components/magic-bento), TypeScript + Tailwind variant, fetched 2026-11-08.
- Edits: a first line `// @ts-nocheck`, and `@hugeicons` replaced by a small lucide-react shim
  (marked `// Genesy edit`), so no new dependency. Nothing else.
- Licence: MIT + Commons Clause, see `LICENSE.md`.
- Used by the overview (`app/(app)/page.tsx`, through `island`, wide screens only) and the
  dev-only preview `app/(app)/design/overview`. Colours come from the `.magic-bento` block in
  `app/office.css`; the overview's own tweaks are in its `.overview-bento` block there.
