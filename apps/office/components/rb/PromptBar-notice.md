PromptBar: React Bits TypeScript + Tailwind source, fetched 2026-11-08:
https://github.com/DavidHDev/react-bits/blob/main/src/ts-tailwind/Micro/PromptBar/PromptBar.tsx

Copyright (c) 2026 David Haz. MIT + Commons Clause License Condition v1.0.
Full license in LICENSE.md. Product embedding is allowed; selling or redistributing the components themselves is not.

Office compatibility header `// @ts-nocheck` as on every file here.
Edits, both marked `// Genesy edit:` in the file:

1. `@hugeicons` replaced by a small lucide-react shim at the top (no new dependency; same call sites).
2. Three optional props, upstream behaviour unchanged when they are left out:
   - `labels` — translated accessible names (`prompt`, `add`, `sources`, `send`, `stop`); English defaults.
   - `ready` — arms Send without text, for files staged outside the bar (camera, drop, paste).
   - `onSend` may return `false` (or a promise of it) to keep the draft, so a refused or failed send never loses typed text.
     Customization lives in components/voice/ (props and the palette overrides in app/office.css).
