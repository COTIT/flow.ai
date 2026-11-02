# React Bits components

Copied unchanged from `apps/landing/components/rb/` (itself from [React Bits](https://reactbits.dev),
TypeScript + Tailwind variants, fetched 2026-11-02). Licence: MIT + Commons Clause, see `LICENSE.md`:
use on our site is allowed; selling or redistributing the components themselves is not.

Only `components/motion.tsx` imports them, through `island` from `@genesy/ui`: lazily, on the client,
and only when the visitor allows motion. The only change is a first line `// @ts-nocheck`. Do not
edit them otherwise; wrap them instead, so a later copy can replace them.
