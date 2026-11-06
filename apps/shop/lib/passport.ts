/** Batch QR code: 10 Crockford base32 characters (no I, L, O, U). */
export const isPublicCode = (code: string) =>
  /^[0-9A-HJKMNP-TV-Z]{10}$/.test(code)

/** Product and order ids are UUIDs; anything else would change the API path the server calls. */
export const isUuid = (id: string) =>
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)

/** What a buyer types from the label ("7k3m 9qx2-wd") as the code the API knows. */
export const normalizeCode = (raw: string) =>
  raw.toUpperCase().replace(/[^0-9A-Z]/g, "")
