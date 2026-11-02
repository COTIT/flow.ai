# React Bits components

Copied unchanged from apps/landing/components/rb (itself from [React Bits](https://reactbits.dev),
TypeScript + Tailwind variant, fetched 2026-11-02). Licence: MIT + Commons Clause, see `LICENSE.md`:
use in our product is allowed; selling or redistributing the components themselves is not.

Only the overview imports them, through `island` from @genesy/ui, so the code is fetched lazily
and only when the user allows motion. The only change is a first line `// @ts-nocheck`. Do not edit
them otherwise; wrap them instead, so a later copy can replace them.
