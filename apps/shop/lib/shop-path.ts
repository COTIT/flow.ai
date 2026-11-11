import { isUuid } from "./passport"

export const SHOP_HEADER = "x-genesy-shop"

/** A connection is public, but only a UUID may become part of an upstream path. */
export function shopBase(connection: string | null): string {
  return connection && isUuid(connection)
    ? `/s/${connection.toLowerCase()}`
    : ""
}

export function shopPath(base: string, href: string): string {
  if (!base || !href.startsWith("/") || href.startsWith("//")) return href
  if (href === base || href.startsWith(`${base}/`)) return href
  return `${base}${href === "/" ? "" : href}`
}
