import "server-only"
import { formatMoney } from "@genesy/ui/lib/money"
import { VoiceError } from "./server"
import { timingWords, type ScheduleTiming } from "../scheduled-reports"
import en from "../../messages/en.json"
import nl from "../../messages/nl.json"
import sw from "../../messages/sw.json"

const scheduledText: Record<string, typeof en.scheduled> = {
  en: en.scheduled,
  nl: nl.scheduled,
  sw: sw.scheduled,
}

/** A scheduled report request in words: which report, when (in the workspace zone), to whom. */
function scheduledWords(
  body: Record<string, unknown>,
  locale: string,
  zone?: string
) {
  const text = scheduledText[locale] ?? scheduledText.en!
  const timing: ScheduleTiming = {
    frequency: String(body.frequency),
    weekday: typeof body.weekday === "number" ? body.weekday : null,
    day_of_month:
      typeof body.day_of_month === "number" ? body.day_of_month : null,
    hour: typeof body.hour === "number" ? body.hour : 0,
  }
  const words = timingWords(timing, locale)
  const pattern =
    text.every[words.key.split(".").pop() as keyof typeof text.every]
  const sentence = pattern
    .replace("{day}", words.values.day)
    .replace("{time}", words.values.time)
  return {
    report:
      text.reports[String(body.report) as keyof typeof text.reports] ??
      String(body.report),
    schedule: `${sentence} (${zone ?? "UTC"})`,
    recipient: text.onlyYou,
  }
}

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
      // A currency record reads as its code on the card, not as its own fields.
      if (key === "currency" && unit && object(amount)) return [key, unit.code]
      if (!key.endsWith("_minor"))
        return [key, project(amount, locale, unit, depth + 1)]
      // An absent old amount (e.g. no wholesale price yet) stays empty, never zero.
      if (amount === null) return [key.slice(0, -6), null]
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
  locale: string,
  zone?: string
): unknown {
  if (operationId === "scheduled_reports_create" && object(body))
    return scheduledWords(body, locale, zone)
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

const warningText: Record<
  string,
  { up: string; down: string; fields: Record<string, string> }
> = {
  en: {
    up: "{field} goes from {before} to {after}, more than 10 times the current amount. Check the amount and its unit.",
    down: "{field} goes from {before} to {after}, less than a tenth of the current amount. Check the amount and its unit.",
    fields: {
      price: "Price",
      gross_price: "Price incl. VAT",
      wholesale_price: "Wholesale price",
    },
  },
  nl: {
    up: "{field} gaat van {before} naar {after}, meer dan 10 keer het huidige bedrag. Controleer het bedrag en de eenheid.",
    down: "{field} gaat van {before} naar {after}, minder dan een tiende van het huidige bedrag. Controleer het bedrag en de eenheid.",
    fields: {
      price: "Prijs",
      gross_price: "Prijs incl. btw",
      wholesale_price: "Groothandelsprijs",
    },
  },
  sw: {
    up: "{field} inabadilika kutoka {before} hadi {after}, zaidi ya mara 10 ya kiasi cha sasa. Kagua kiasi na kipimo chake.",
    down: "{field} inabadilika kutoka {before} hadi {after}, chini ya sehemu ya kumi ya kiasi cha sasa. Kagua kiasi na kipimo chake.",
    fields: {
      price: "Bei",
      gross_price: "Bei pamoja na VAT",
      wholesale_price: "Bei ya jumla",
    },
  },
}

/** The API flags large money changes; this only words them for the card in the user's language. */
export function reviewWarnings(snapshot: unknown, locale: string): string[] {
  const facts =
    object(snapshot) && object(snapshot.facts) ? snapshot.facts : undefined
  if (!facts || !Array.isArray(facts.changes)) return []
  const text = warningText[locale] ?? warningText.en!
  return facts.changes.flatMap((change) => {
    if (!object(change) || change.warning !== "large_change") return []
    const shown = project(change, locale) as Record<string, unknown>
    const field = String(change.field)
    return [
      (Number(change.change_bp) < 0 ? text.down : text.up)
        .replace("{field}", text.fields[field] ?? field.replaceAll("_", " "))
        .replace("{before}", String(shown.before ?? "—"))
        .replace("{after}", String(shown.after ?? "—")),
    ]
  })
}
