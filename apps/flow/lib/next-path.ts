/**
 * Where to go after logging in: `next` only when it is a path on this origin (never another
 * site, a scheme, or the login page itself); anything else goes home.
 */
export function safeNext(next: string | null | undefined, origin: string): string {
  // A leading "//" or any backslash is read by browsers as another host; control characters are dropped by URL parsing.
  const control = [...(next ?? "")].some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)
  if (!next?.startsWith("/") || next.startsWith("//") || next.includes("\\") || control) return "/"
  try {
    const url = new URL(next, origin)
    if (url.origin !== origin || url.pathname === "/login") return "/"
    return url.pathname + url.search + url.hash
  } catch {
    return "/"
  }
}
