import "server-only"
import { formatMoney } from "@genesy/ui/lib/money"
import { VoiceError } from "./server"

type Currency = { code: string; decimals: number }
function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}
function currency(
  value: Record<string, unknown>,
  inherited?: Currency
): Currency | undefined {
  const code = object(value.currency) ? value.currency.code : value.currency
  const decimals = object(value.currency)
    ? value.currency.decimals
    : value.decimals
  return typeof code === "string" &&
    /^[A-Z]{3}$/i.test(code) &&
    typeof decimals === "number" &&
    Number.isInteger(decimals) &&
    decimals >= 0 &&
    decimals <= 8
    ? { code: code.toUpperCase(), decimals }
    : inherited
}
function project(
  value: unknown,
  locale: string,
  inherited?: Currency,
  depth = 0
): unknown {
  if (
    depth > 32 ||
    (typeof value === "number" &&
      (!Number.isFinite(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER))
  )
    throw new VoiceError("Review values cannot be displayed exactly.", 409)
  if (Array.isArray(value))
    return value.map((child) => project(child, locale, inherited, depth + 1))
  if (!object(value)) return value
  const unit = currency(value, inherited)
  return Object.fromEntries(
    Object.entries(value).map(([key, amount]) => {
      if (!key.endsWith("_minor"))
        return [key, project(amount, locale, unit, depth + 1)]
      if (!unit || typeof amount !== "number" || !Number.isSafeInteger(amount))
        throw new VoiceError("Review money cannot be displayed exactly.", 409)
      return [
        key.slice(0, -6),
        formatMoney(amount, unit.decimals, unit.code, locale),
      ]
    })
  )
}

/** Requested order amounts use display currency; the signed request keeps integer minor units. */
export function reviewChanges(
  body: unknown,
  snapshot: unknown,
  operationId: string,
  locale: string
): unknown {
  if (
    !operationId.startsWith("orders_") ||
    !object(body) ||
    !Array.isArray(body.lines)
  )
    return body
  const facts =
    object(snapshot) && object(snapshot.facts) ? snapshot.facts : undefined
  const priced =
    facts && object(facts.priced)
      ? facts.priced
      : facts && object(facts.locked)
        ? facts.locked
        : undefined
  const unit = priced ? currency(priced) : undefined
  return {
    ...body,
    lines: body.lines.map((line) => {
      if (!object(line) || typeof line.unit_price !== "number") return line
      if (!Number.isSafeInteger(line.unit_price))
        throw new VoiceError("Review money cannot be displayed exactly.", 409)
      const minor =
        (
          {
            en: "minor units",
            nl: "kleinste valuta-eenheden",
            sw: "vitengo vidogo vya sarafu",
          } as Record<string, string>
        )[locale] ?? "minor units"
      return {
        ...line,
        unit_price: unit
          ? formatMoney(line.unit_price, unit.decimals, unit.code, locale)
          : `${line.unit_price} ${minor}`,
      }
    }),
  }
}

/** Display immutable backend facts; hidden fields stay private and minor units stay exact. */
export function reviewFacts(
  snapshot: unknown,
  operationId: string,
  locale: string
): Record<string, unknown> | undefined {
  if (snapshot == null || (object(snapshot) && !Object.keys(snapshot).length))
    return undefined
  if (!object(snapshot))
    throw new VoiceError("Review facts are unavailable.", 409)
  if (snapshot.factsHiddenByPermission === true)
    return {
      notice:
        (
          {
            en: "Some current values are hidden by your permissions. The backend still checks them before executing.",
            nl: "Je rechten verbergen enkele huidige waarden. De backend controleert deze nog steeds vóór uitvoering.",
            sw: "Baadhi ya thamani za sasa zimefichwa kulingana na ruhusa zako. Mfumo bado huzikagua kabla ya kutekeleza.",
          } as Record<string, string>
        )[locale] ?? "Current values are hidden by your permissions.",
    }
  if (
    snapshot.version !== 1 ||
    snapshot.operation_id !== operationId ||
    !object(snapshot.facts)
  )
    throw new VoiceError(
      "Review facts do not match this action. Prepare a new review.",
      409
    )
  return project(snapshot.facts, locale) as Record<string, unknown>
}
