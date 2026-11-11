AnimatedList: React Bits TypeScript + Tailwind source, fetched 2026-11-08:
https://github.com/DavidHDev/react-bits/blob/main/src/ts-tailwind/Components/AnimatedList/AnimatedList.tsx

Copyright (c) 2026 David Haz. MIT + Commons Clause License Condition v1.0.
Full license in LICENSE.md. Product embedding is allowed; selling or redistributing the components themselves is not.

Office compatibility header `// @ts-nocheck` as on every file here.
The only edit: `@hugeicons` replaced by a small lucide-react shim at the top (no new dependency; same call sites), marked `// Genesy edit:`.
Customization lives in the hosts that render it (props) and in `src/styles/voice.css` (palette overrides). Moved here from apps/office/components/rb on 2026-11-10 so Flow can share it.
