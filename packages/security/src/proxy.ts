import { NextResponse, type NextRequest } from "next/server"

import { contentSecurityPolicy } from "./headers"

/**
 * An app's `proxy.ts` handler: a fresh nonce per request, passed to Next (which tags
 * its scripts with it) and to the layout as `x-nonce`, plus the CSP response header.
 */
export function cspProxy(
  options: {
    frameAncestors?: string
    imgSrc?: string
    voicePath?: string
    voice?: boolean
  } = {}
) {
  return function proxy(
    request: NextRequest,
    destination?: URL,
    forwarded?: Headers
  ) {
    const nonce = Buffer.from(crypto.randomUUID()).toString("base64")
    const csp = contentSecurityPolicy(nonce, {
      ...options,
      voice:
        options.voice === true ||
        options.voicePath === request.nextUrl.pathname,
      dev: process.env.NODE_ENV === "development",
    })

    const headers = new Headers(forwarded ?? request.headers)
    headers.set("x-nonce", nonce)
    headers.set("Content-Security-Policy", csp)

    const response = destination
      ? NextResponse.rewrite(destination, { request: { headers } })
      : NextResponse.next({ request: { headers } })
    response.headers.set("Content-Security-Policy", csp)
    return response
  }
}
