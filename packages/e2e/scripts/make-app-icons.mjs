// Renders the home-screen icons (PWA) for office, till and field from the logo SVGs.
// Run from the repo root after a logo change: node packages/e2e/scripts/make-app-icons.mjs
// Uses the Playwright Chromium the e2e suite already installs; outputs are committed.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { chromium } from "@playwright/test"

const root = join(import.meta.dirname, "../../..")
const read = (p) => readFileSync(join(root, p), "utf8").replace(/^<\?xml[^>]*\?>/, "")

const bars = read("apps/office/app/icon.svg")
const apps = {
  // Gold bars on navy.
  office: { background: "#0a183a", logo: bars },
  // Navy bars on gold, so the two apps differ at a glance on the phone.
  till: {
    background: "#efc977",
    logo: bars.replace(/fill:url\(#linear-gradient(-\d)?\)/g, "fill:#0a183a"),
  },
  // The Ruaha drop only: its first three paths (x 0-171), centred in a 256 square; the
  // wordmark paths are dropped. On the field app's cream.
  field: {
    background: "#F5F3E5",
    logo: (() => {
      let n = 0
      return read("apps/field/public/ruaha-logo.svg")
        .replace(/viewBox="[^"]*"/, 'viewBox="-42.5 0 256 256"')
        .replace(/<path[^>]*\/>/g, (path) => (++n <= 3 ? path : ""))
    })(),
  },
}

// [file, size, share of the tile the logo may fill]. Maskable keeps to the 80% safe circle.
const icons = [
  ["icon-192.png", 192, 0.62],
  ["icon-512.png", 512, 0.62],
  ["icon-maskable-512.png", 512, 0.5],
  ["apple-touch-icon.png", 180, 0.62],
]

const browser = await chromium.launch()
const page = await browser.newPage()
for (const [app, { background, logo }] of Object.entries(apps)) {
  const out = join(root, "apps", app, "public/icons")
  mkdirSync(out, { recursive: true })
  for (const [file, size, share] of icons) {
    await page.setViewportSize({ width: size, height: size })
    const box = Math.round(size * share)
    await page.setContent(
      `<body style="margin:0;width:${size}px;height:${size}px;background:${background};display:grid;place-items:center">` +
        `<div style="width:${box}px;height:${box}px">${logo.replace("<svg", '<svg width="100%" height="100%"')}</div></body>`
    )
    writeFileSync(join(out, file), await page.screenshot({ type: "png" }))
  }
  console.log(`${app}: ${icons.length} icons in apps/${app}/public/icons`)
}
await browser.close()
