/**
 * Voucher codes: ten Crockford base32 symbols, issued by `app_survey_submit`.
 *
 * The QR code carries `R360V:<code>` so a scanner pointed at any other QR code
 * gets "not a voucher" rather than a lookup. Staff can also type the code; the
 * letters Crockford never issues (I, L, O) are read as the digits they are
 * mistaken for. Mirrors `app_voucher_normalize`, which is what the lookup
 * actually trusts; this copy only decides whether a scan is worth sending.
 */

const QR_PREFIX = 'R360V:'
const CODE = /^[0-9A-HJKMNP-TV-Z]{10}$/

export function normalizeVoucherCode(input: string): string | null {
  const bare = input
    .trim()
    .replace(/^R360V:/i, '')
    .toUpperCase()
    .replace(/[^0-9A-Z]/g, '')
    .replace(/O/g, '0')
    .replace(/[IL]/g, '1')
  return CODE.test(bare) ? bare : null
}

/** `K7QXM-2PA9D`: two halves, easier to read aloud and to type. */
export function formatVoucherCode(code: string): string {
  return `${code.slice(0, 5)}-${code.slice(5)}`
}

export function voucherQrPayload(code: string): string {
  return `${QR_PREFIX}${code}`
}
