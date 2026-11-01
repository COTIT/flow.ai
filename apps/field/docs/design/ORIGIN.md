# Where these files came from

The designer's reply to `docs/design-handoff.md`, delivered as
`Design feedback questions.zip` on 13 September 2026. The filename is a
misnomer — the bundle contains no questions, it contains the redesign.

`README.md` and `IMPLEMENTATION_PROMPT.md` are the designer's own, unedited.
`tokens.css` is kept here as delivered; the merged, live copy is
`src/styles/globals.css`.

The three `.dc.html` files are design **references**, not production code. Open
them in a browser — `support.js` sits beside them and no server is needed.
`Ruaha 360 - Current UI.dc.html` is the "before".

## One correction to the bundle

The three brand lockups in the bundle's own `assets/` folder are unusable: each
has an empty `<defs>`, no `<style>` block and no `fill` on any of its 14 paths,
so all three render as a solid black silhouette. The bundle's README notices
this only for `ruaha-logo-white.svg`; it is true of all three. The likely cause
is the C2PA re-save that added a ~8 kB metadata blob to each file and dropped
the `<style>` element from `<defs>` on the way.

The originals in the brand book's own `Style guide/Logo/` folder are intact:

```
Logo.svg              .cls-1{fill:#1d70b7}  .cls-2{fill:#93c01f}
Logo_for_dark_bg.svg  .cls-1{fill:#fff}  .cls-2{fill:#1d70b7}  .cls-3{fill:#93c01f}
Logo_white.svg        .cls-1{fill:#fff}
```

Those are what ship, as `public/ruaha-logo*.svg`, and they are what
`assets/` here holds too so the design references render as intended. They are
also a third of the size, having no C2PA blob.

Note that the wordmark paths in `Logo.svg` carry no class and so render black,
not `--deep` navy. That is the supplied lockup and it is what ships; if the
brand book intends otherwise, the fix belongs in the brand file, not in CSS.

## The fonts

`assets/fonts/` holds the four Poppins weights as supplied, so the references
render in the right type. The application does not use them: it self-hosts the
same four weights, latin subset only, through `@fontsource/poppins` — see the
comment at the top of `src/styles/globals.css`. No new font ships.
