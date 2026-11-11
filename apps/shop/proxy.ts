import { NextResponse, type NextRequest } from "next/server"
import { LOCALE_COOKIE, pickLocale } from "@genesy/i18n/locales"
import { securityHeaders } from "@genesy/security/headers"
import { cspProxy } from "@genesy/security/proxy"

import en from "./messages/en.json"
import nl from "./messages/nl.json"
import sw from "./messages/sw.json"
import { SHOP_HEADER, shopBase } from "./lib/shop-path"

// shortcut: frames refused everywhere; pass { frameAncestors } here once a shop page must be embedded.
// FILES_ORIGIN: the file storage host serving the signed passport images (one exact origin).
const secured = cspProxy({ imgSrc: process.env.FILES_ORIGIN })
const messages = { en, nl, sw }

function html(value: string) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;")
}

/** Return a real unavailable status before catalog rendering can fail. */
export function proxy(request: NextRequest) {
  const path = request.nextUrl.pathname
  const forwarded = new Headers(request.headers)
  // The visible URL is the only source of scope; clients cannot override it with headers.
  forwarded.delete(SHOP_HEADER)
  if (path === "/s" || path.startsWith("/s/")) {
    const match = /^\/s\/([^/]+)(\/.*)?$/.exec(path)
    const base = shopBase(match?.[1] ?? null)
    const target = match?.[2] ?? "/"
    if (
      !base ||
      !/^\/(?:shop|cart|checkout|b(?:\/[^/]+)?|products\/[^/]+|orders\/[^/]+)?$/.test(
        target
      )
    )
      return new NextResponse("Shop page not found", { status: 404 })
    forwarded.set(SHOP_HEADER, base.slice(3))
    const destination = new URL(request.url)
    destination.pathname = target
    return secured(request, destination, forwarded)
  }
  const needsCatalog =
    ["/", "/shop", "/checkout"].includes(path) ||
    ["/products/", "/orders/", "/b/"].some((prefix) => path.startsWith(prefix))
  if (!process.env.STOREFRONT_APP_KEY?.trim() && needsCatalog) {
    const locale = pickLocale(request.cookies.get(LOCALE_COOKIE)?.value, "en")
    const text = messages[locale]
    const headers = new Headers(
      securityHeaders.map(({ key, value }) => [key, value])
    )
    headers.set(
      "Content-Security-Policy",
      secured(request).headers.get("Content-Security-Policy")!
    )
    headers.set("Content-Type", "text/html; charset=utf-8")
    headers.set("Cache-Control", "no-store")
    headers.set("Retry-After", "60")
    return new NextResponse(
      request.method === "HEAD"
        ? null
        : `<!doctype html>
<html lang="${locale}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${html(text.errors.unavailableTitle)} · ${html(text.app.title)}</title>
<style>
:root{color-scheme:light dark;--ground:#eef3f5;--surface:#ffffffcc;--ink:#0a183a;--muted:#456087;--ring:#01909b}
*{box-sizing:border-box}body{margin:0;padding:24px;background:var(--ground);color:var(--ink);font:400 1rem/1.6 Poppins,system-ui,sans-serif}main{min-height:calc(100svh - 48px);display:grid;place-items:center}article{width:100%;max-width:40rem;padding:24px;border-radius:20px;background:var(--surface)}h1{font-size:1.5rem;line-height:1.3;font-weight:600}p{max-width:60ch}.brand{font-size:.875rem;color:var(--muted)}nav{display:flex;flex-wrap:wrap;gap:12px;margin-top:24px}a{display:inline-flex;align-items:center;min-height:44px;padding:0 16px;border:1px solid var(--muted);border-radius:12px;color:var(--ink);text-decoration:none}a:hover{background:var(--ground)}a:focus-visible{outline:2px solid var(--ring);outline-offset:4px}
@media(prefers-color-scheme:dark){:root{--ground:#0a183a;--surface:#142447;--ink:#f2f6f7;--muted:#b9c7dd;--ring:#efc977}}
</style></head><body><main><article aria-labelledby="unavailable"><p class="brand">${html(text.app.title)}</p><h1 id="unavailable">${html(text.errors.unavailableTitle)}</h1><p>${html(text.errors.unavailableBody)}</p><nav><a href="">${html(text.errors.retry)}</a><a href="/contact">${html(text.nav.contact)}</a></nav></article></main></body></html>`,
      { status: 503, headers }
    )
  }
  // Keep configured prefetches on their existing path without adding nonce headers.
  if (
    request.headers.has("next-router-prefetch") ||
    request.headers.get("purpose") === "prefetch"
  )
    return NextResponse.next({ request: { headers: forwarded } })
  return secured(request, undefined, forwarded)
}

// Pages only: static files, images and the /v1 forward to the backend skip it.
export const config = {
  matcher: [
    {
      source: "/((?!v1/|_next/static|_next/image|favicon.ico).*)",
    },
  ],
}
