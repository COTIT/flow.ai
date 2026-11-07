import { expect, type Page } from "@playwright/test"

import type { Allow } from "./test"

/** Chrome's console line when the CSP stops an inline handler; its tail names the nonce. */
export const blockedHandler: Allow = {
  consolePrefix: [
    "Executing inline event handler violates the following Content Security Policy directive",
  ],
}

/**
 * Loads `path` twice and checks the headers every Next app sends (packages/security): a CSP
 * whose script nonce is new on each load, framing refused, plus nosniff and Referrer-Policy.
 */
export async function expectSecurityHeaders(
  page: Page,
  path: string,
  connectOrigins: readonly string[] = []
) {
  const nonces: string[] = []
  for (let load = 0; load < 2; load++) {
    const headers = (await page.goto(path))!.headers()
    expect(headers["x-content-type-options"]).toBe("nosniff")
    expect(headers["referrer-policy"]).toBe("strict-origin-when-cross-origin")
    const csp = Object.fromEntries(
      headers["content-security-policy"]!.split(";").map((d) => {
        const [name, ...sources] = d.trim().split(/\s+/)
        return [name!, sources]
      })
    )
    expect(csp["default-src"]).toEqual(["'self'"])
    expect(csp["frame-ancestors"]).toEqual(["'none'"])
    expect(csp["object-src"]).toEqual(["'none'"])
    expect(csp["base-uri"]).toEqual(["'self'"])
    expect(csp["form-action"]).toEqual(["'self'"])
    expect(csp["connect-src"]).toEqual(["'self'", ...connectOrigins])
    const scripts = csp["script-src"]!
    expect(scripts).toContain("'strict-dynamic'")
    expect(scripts).not.toContain("'unsafe-inline'")
    expect(scripts).not.toContain("'unsafe-eval'")
    const nonce = scripts.find((s) => s.startsWith("'nonce-"))
    expect(nonce, "script nonce").toBeTruthy()
    nonces.push(nonce!)
  }
  expect(nonces[0], "nonce reused across loads").not.toBe(nonces[1])
}

/**
 * Injects markup the way an XSS would (an inline `onerror`) and proves the CSP stops it:
 * the browser reports a script violation and the handler never runs. Needs `blockedHandler`.
 */
export async function expectInjectedScriptBlocked(page: Page) {
  const violation = page.evaluate(
    () =>
      new Promise<string>((resolve) =>
        document.addEventListener(
          "securitypolicyviolation",
          (e) => resolve(e.effectiveDirective),
          { once: true }
        )
      )
  )
  await page.evaluate(() => {
    const box = document.createElement("div")
    box.innerHTML = '<img src="data:," onerror="window.__injected = true">'
    document.body.append(box)
  })
  expect(await violation).toMatch(/^script-src/)
  expect(
    await page.evaluate(() => "__injected" in window),
    "injected handler ran"
  ).toBe(false)
}
