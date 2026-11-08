import type { Metadata, MetadataRoute, Viewport } from "next"

/** The web app manifest that lets a phone install the app and open it full screen. */
export function webManifest({
  name,
  shortName,
  startUrl = "/",
  themeColor,
  backgroundColor,
}: {
  name: string
  /** Shown under the home-screen icon; keep it to about 12 characters. */
  shortName: string
  /** A page that answers 200 itself: a redirect here breaks the offline start. */
  startUrl?: string
  themeColor: string
  backgroundColor: string
}): MetadataRoute.Manifest {
  return {
    id: "/",
    name,
    short_name: shortName,
    start_url: startUrl,
    scope: "/",
    display: "standalone",
    theme_color: themeColor,
    background_color: backgroundColor,
    icons: [
      { src: "/icons/icon-192.png", sizes: "192x192", type: "image/png" },
      { src: "/icons/icon-512.png", sizes: "512x512", type: "image/png" },
      {
        src: "/icons/icon-maskable-512.png",
        sizes: "512x512",
        type: "image/png",
        purpose: "maskable",
      },
    ],
  }
}

/** iPhone home-screen tags (iOS reads these, not the manifest). Default status bar: it never covers the page. */
export function pwaMetadata(title: string): Pick<Metadata, "appleWebApp" | "icons"> {
  return {
    appleWebApp: { capable: true, title, statusBarStyle: "default" },
    icons: { apple: "/icons/apple-touch-icon.png" },
  }
}

/** Browser bar colour, matching the light and dark page backgrounds of packages/ui. */
export const pwaViewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#ffffff" },
    { media: "(prefers-color-scheme: dark)", color: "#0a183a" },
  ],
}

/**
 * The service worker script, served at /sw.js. `precache` pages (and the build files they
 * load) are kept at install so the app opens with no network; other pages are kept as visited.
 * Pages carry no account data (it comes from /v1 in the browser), and /v1 is never touched.
 * `version` changes per build, so an update drops old pages; hashed build files stay.
 */
export function serviceWorkerSource({
  precache,
  version,
}: {
  precache: string[]
  version: string
}): string {
  return (
    `const PRECACHE = ${JSON.stringify(precache)}\n` +
    `const PAGES = ${JSON.stringify(`pages-${version}`)}\n` +
    WORKER
  )
}

// Plain JS for the browser; no template literals inside, so String.raw keeps it verbatim.
const WORKER = String.raw`
// shortcut: build files are never trimmed; add an age limit if the cache passes a few MB.
const STATIC = "static"
const ASSET = /\/_next\/static\/[^"'\s\\)]+/g

const key = (u) => {
  const url = new URL(u, self.location.origin)
  return url.origin + url.pathname
}

self.addEventListener("install", (event) => {
  event.waitUntil(precache().then(() => self.skipWaiting()))
})

async function precache() {
  const pages = await caches.open(PAGES)
  const files = await caches.open(STATIC)
  await Promise.all(
    PRECACHE.map(async (path) => {
      const res = await fetch(path)
      if (!res.ok) throw new Error("precache " + path + ": " + res.status)
      const html = await res.clone().text()
      await pages.put(key(path), res)
      const assets = [...new Set(html.match(ASSET) || [])]
      await Promise.all(assets.map((a) => keepFile(files, a)))
    })
  )
}

async function keepFile(files, path) {
  if (await files.match(key(path))) return
  const res = await fetch(path)
  if (res.ok) await files.put(key(path), res)
}

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((n) => n.startsWith("pages-") && n !== PAGES)
            .map((n) => caches.delete(n))
        )
      )
      .then(() => self.clients.claim())
  )
})

self.addEventListener("fetch", (event) => {
  const req = event.request
  const url = new URL(req.url)
  // API answers are never kept: a cached answer would outlive a revoked membership.
  if (req.method !== "GET" || url.origin !== self.location.origin) return
  if (url.pathname === "/v1" || url.pathname.startsWith("/v1/")) return
  if (url.pathname.startsWith("/_next/static/")) event.respondWith(cacheFirst(req))
  else if (req.mode === "navigate") event.respondWith(networkFirst(req))
  // Next's in-app page fetch: offline, the kept HTML (not flight data) makes Next load the page in full.
  else if (req.headers.get("RSC") === "1") event.respondWith(fetch(req).catch(() => kept(req)))
})

async function kept(req) {
  const pages = await caches.open(PAGES)
  return (
    (await pages.match(key(req.url))) ||
    (await pages.match(key("/offline"))) ||
    Response.error()
  )
}

async function cacheFirst(req) {
  const files = await caches.open(STATIC)
  const hit = await files.match(key(req.url))
  if (hit) return hit
  const res = await fetch(req)
  if (res.ok) await files.put(key(req.url), res.clone())
  return res
}

async function networkFirst(req) {
  const pages = await caches.open(PAGES)
  try {
    const res = await fetch(req)
    if (res.ok && !res.redirected) await pages.put(key(req.url), res.clone())
    return res
  } catch (error) {
    return kept(req)
  }
}
`
