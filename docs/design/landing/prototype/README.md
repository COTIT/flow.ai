# genesy.app landing — React prototype (design only)

A throwaway prototype for choosing a design. **This is not the app.**
- It is not part of the pnpm workspace or the turbo pipeline.
- It has its own `node_modules` and `package-lock.json`, installed with npm.
- Nothing under `apps/` or `packages/` depends on it.

## Run

```bash
cd docs/design/landing/prototype && npm ci && npm run dev
```

Then open http://localhost:4810/. Add these to the address:
- `?d=a`, `?d=b` or `?d=c` picks the direction.
- `&theme=light` or `&theme=dark` picks the theme.

The pill at the bottom left switches between them. To see the calm version, switch on "reduce motion" in the device settings and reload.

## How it is built

- **The three static mockups are the base.** `pages/{a,b,c}.html` and `.css` were extracted from `../a-navy-gold`, `../b-mist-journey` and `../c-passport-first`. They already show the final, still state.
- **React Bits components fill marked spots ("islands").**
  - These are spots marked `data-rb="…"` in the page, mapped to components in `src/islands.tsx`.
  - They are used only when motion is allowed. With reduced motion, the static page stays as it is.
- **The scroll journey is unchanged from round 2.** `src/motion.ts` is a TypeScript port of `../assets/motion.js`.
- **React Bits source:** `src/rb/*.tsx` is the TS + Tailwind variant from `https://reactbits.dev/r/<Name>-TS-TW.json`, copied unchanged. `src/rb/LICENSE.md` holds their licence.

## Where each React Bits component fits

| Component | Direction · section | Why |
|---|---|---|
| LightRays (WebGL) | A · hero background | Gold light falling on the navy, like the guide's cover; draws the eye to the headline |
| SplitText (GSAP) | A · headline | Words rise one by one, so the promise reads as a sentence being said |
| ShinyText | A · hero tag | A gold sheen on "Farm to kitchen, in the open", matching the logo gradient |
| StarBorder | A · main button | A gold light circles the "Talk to us" button, the one call to action |
| CountUp (motion) | A · hero facts; A/B/C · passport numbers | Numbers tick up, so the passport feels live |
| SpotlightCard | A · programmes and brands panels | A gold spotlight follows the pointer over the two audience panels |
| AnimatedContent (GSAP) | A · passport card | The card lifts and tilts into place as you reach it |
| ScrollReveal (GSAP) | A · "Plain rules, kept by the system." | The promise sharpens word by word as you scroll |
| Particles (WebGL) | B · hero background | Drifting gold specks, like pollen or bees over the landscape |
| BlurText (motion) | B · headline | The headline comes into focus, a soft, warm entrance |
| RotatingText (motion) | B · hero tag | "Traced from the hive / harvest / batch / passport / shop / passbook": the journey in one line |
| LogoLoop | B · strip under the hero | A slow loop of what Genesy does (offline, three languages, passport, one stock, passbook, consent, exact money). There are no partner logos yet |
| GlareHover | B · audience cards | A gold glare across each card on hover |
| FadeContent (GSAP) | B · section headings | Headings fade in from a blur |
| DotGrid | C · hero background | A quiet grid of dots that react to the pointer, like a QR code coming alive |
| TextType (GSAP) | C · headline | "This jar can tell you who made it." types itself out, as if the jar is speaking |
| GradientText | C · "The whole journey is inside." | The brand gold gradient slowly moving through the key line |
| ScrollFloat (GSAP) | C · section headings | Letters float into place as each section arrives |
| Magnet + ClickSpark | C · "Email us" button | The button leans toward the pointer and sparks gold when clicked |
| *(custom, round 2)* | A/B/C · scroll journey | React Bits has no story-scroller. The pinned six-stage journey (hive → passbook) stays our own SVG |

## Added packages (exact versions, all published at least 2 weeks before 2026-11-02)

| Package | Version | Licence | Weight (min + gzip, as used) |
|---|---|---|---|
| react, react-dom | 19.3.1 | MIT | ~74 KB together |
| gsap (core + ScrollTrigger + SplitText) | 3.15.0 | GSAP Standard License (free, including commercial sites) | ~51 KB with @gsap/react |
| @gsap/react | 2.1.2 | GSAP Standard License | ~1 KB |
| motion | 12.43.0 | MIT | ~50 KB (motion/react, the parts used) |
| React Bits components (20 files, copied in) | registry, 2026-11-02 | MIT + Commons Clause | ~59 KB with app code and scenes |
| vite, @vitejs/plugin-react, tailwindcss, @tailwindcss/vite, typescript, @types/* | 8.3.4, 6.1.2, 4.3.3, 4.3.3, 5.9.3, 19.3.0 | MIT / Apache-2.0 | build-time only |

- **Whole prototype:** 234 KB gzip of JavaScript, holding all three directions at once.
- **What the real page would carry:** only the chosen direction, with its WebGL background loaded late. Expect roughly 140–170 KB.
- Note: the React Bits registry asks for `motion@^12.23.12`. The newest `motion` is 14.x, so the prototype pins the 12.x line the components were written for.

## Licences in plain words

- **React Bits: MIT + Commons Clause.**
  - We may use, change and publish the components as part of our website, including for commercial purposes.
  - We may not sell or redistribute the components themselves, for example as a component pack or template.
  - Keep the licence notice with the copied files (`src/rb/LICENSE.md`). No visible credit on the site is required.
- **GSAP: Standard License (Webflow).**
  - Free for everyone, all plugins included, also on commercial sites.
  - Not allowed: building a visual no-code animation tool that competes with Webflow, or removing GSAP's notices.
- **motion, React, Vite, Tailwind:** MIT. TypeScript: Apache-2.0.

## Accessibility and performance

- **Reduced motion:** no React Bits component is mounted. The static final state is shown, with no pinned scrolling, no loops and no WebGL. All six journey steps are written out as text.
- Most React Bits components also check reduced motion themselves. BlurText and RotatingText do not, which is why the islands are skipped entirely.
- **Phones:** the WebGL backgrounds are capped at 1–1.5× device pixel ratio. The journey animates only transform and opacity.
- **To check on a real mid-range phone before choosing:** the WebGL hero backgrounds (A LightRays, B Particles).
