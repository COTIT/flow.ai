# Web redesign — office, shop, till (design source)

Claude Design canvas: https://claude.ai/artifact/GEEtyDyy6ujsDoDJ9tACPa (private; 4 pages: glass system, office, shop, till).

- `canvas/` — the canvas files as published: `canvas.json` (board layout), `glass.css` (the shared glass system; maps to `packages/ui` tokens under `body.theme-glass`), one `.dc.html` per artboard. Small `*Dark`, `*Phone`, `*Portrait` files only re-mount a screen with other props.
- `parts/` — office/shop page bodies + the shell head/tail they are spliced into (`assemble_*.sh`, paths point at the original scratchpad).

Design only. No app code depends on these files.
