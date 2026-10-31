/**
 * Price from the API (integer minor units) as display text, e.g. 1250, 2, "USD" -> "$12.50".
 * Exact: the decimal string is built with integer maths and Intl formats that string
 * as-is (Intl.NumberFormat v3), so no amount ever passes through a float.
 */
export function formatMoney(
  minor: number | bigint,
  decimals: number,
  currency: string,
  locale: string
): string {
  const units = BigInt(minor)
  const abs = (units < 0n ? -units : units)
    .toString()
    .padStart(decimals + 1, "0")
  const whole = abs.slice(0, abs.length - decimals)
  const decimal = `${units < 0n ? "-" : ""}${whole}${decimals ? `.${abs.slice(-decimals)}` : ""}`
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency,
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
    // TS's lib types lack the string overload; runtimes since Node 19 / 2023 browsers have it.
  }).format(decimal as unknown as number)
}
