import "server-only"
import { createHash } from "node:crypto"
import { assistantRequest } from "./bridge"
import { proposeAction } from "./actions"
import { proposeBatch } from "./batches"
import { prepareAction } from "./catalog"
import { IMPORT_PAGE, priceRows, readSheet, SHEET_TYPES } from "./sheet"
import { VoiceError, type VoiceContext } from "./server"
import type { VoiceOutcome } from "@genesy/voice/lib/client-contract"

export const IMPORT_PRICES_OPERATION = "assistant_import_prices"

/** Rows kept out of a review; `reason` is a fixed key the card words, never shown raw. */
export type NotIncluded = {
  line?: number
  text: string
  reason: string
  note?: string
  candidates?: string[]
}

export const importPricesTool = {
  id: IMPORT_PRICES_OPERATION,
  summary:
    "Update product prices from a shared price-list spreadsheet (.xlsx or .csv). Genesy reads the sheet, matches products and prices itself and shows one review of up to 50 rows; never copy prices yourself. If it returns needs_input, ask which column holds the product and which holds the price. If data.rows.next is set, offer the next part after this review is finished.",
  method: "POST",
  write: true,
  body: {
    type: "object",
    required: ["attachments"],
    properties: {
      attachments: {
        type: "object",
        required: ["sheet"],
        properties: {
          sheet: {
            type: "array",
            minItems: 1,
            maxItems: 1,
            items: { type: "string", format: "uuid" },
          },
        },
      },
      productColumn: { type: "string" },
      priceColumn: { type: "string" },
      fromRow: { type: "integer", minimum: 1 },
    },
  },
}

type Resolved = {
  line: number
  status: string
  product_id?: string
  price?: string
  candidates?: string[]
}

/** Exact bytes the user shared in this session; Genesy never stores the sheet. */
async function sharedSheet(
  handle: string,
  context: VoiceContext,
  files: Map<string, Blob>
) {
  const blob = files.get(handle)
  if (!blob) throw new VoiceError("Share the price list again.")
  const stored = await assistantRequest<{
    state: string
    media_type: string
    size_bytes: number
    sha256?: string
  }>(
    context,
    `/sessions/${context.sessionId}/uploads/${encodeURIComponent(handle)}`
  )
  const bytes = new Uint8Array(await blob.arrayBuffer())
  if (
    stored.state !== "uploaded" ||
    !(SHEET_TYPES as readonly string[]).includes(stored.media_type) ||
    stored.size_bytes !== bytes.length ||
    stored.sha256 !== createHash("sha256").update(bytes).digest("hex")
  )
    throw new VoiceError(
      "The price list changed or belongs to another conversation. Share it again.",
      409
    )
  return readSheet(bytes, stored.media_type)
}

/** One page of the sheet → API matching → one review; nothing is written before approval. */
export async function importPrices(
  args: Record<string, unknown>,
  context: VoiceContext,
  files: Map<string, Blob>
): Promise<VoiceOutcome> {
  const sheetHandles = (args.attachments as { sheet?: unknown } | undefined)
    ?.sheet
  if (
    !Array.isArray(sheetHandles) ||
    sheetHandles.length !== 1 ||
    typeof sheetHandles[0] !== "string"
  )
    throw new VoiceError("Choose one price list to import.")
  const text = (v: unknown) => (typeof v === "string" ? v : undefined)
  const sheet = await sharedSheet(sheetHandles[0], context, files)
  const page = priceRows(sheet, {
    productColumn: text(args.productColumn),
    priceColumn: text(args.priceColumn),
    fromRow: typeof args.fromRow === "number" ? args.fromRow : undefined,
  })
  if ("needsColumns" in page)
    return {
      status: "needs_input",
      message:
        "Ask the user which column holds the product and which holds the price.",
      data: { columns: page.needsColumns },
    }
  const resolved = await assistantRequest<Resolved[]>(
    context,
    `/sessions/${context.sessionId}/price-import`,
    "POST",
    { rows: page.rows }
  )
  const keys = new Map(page.rows.map((row) => [row.line, row.key]))
  const matched = resolved.filter((row) => row.status === "matched")
  const notIncluded: NotIncluded[] = resolved
    .filter((row) => row.status !== "matched")
    .map((row) => ({
      line: row.line,
      text: keys.get(row.line) ?? "",
      reason: row.status,
      ...(row.candidates ? { candidates: row.candidates } : {}),
    }))
  const rows = { from: page.from, to: page.to, total: page.total }
  const calls = matched.map((row) => ({
    operationId: "products_update",
    arguments: {
      path: { product: row.product_id },
      body: { price: row.price },
    },
  }))
  if (!calls.length)
    return {
      status: "completed",
      message:
        "No price in these rows can be changed. Tell the user which rows need fixing.",
      data: { rows: { ...rows, next: page.next }, notIncluded },
    }
  const outcome =
    calls.length === 1
      ? await proposeAction(
          prepareAction(
            calls[0]!.operationId,
            calls[0]!.arguments,
            context.workspaceId
          ),
          context,
          new Map()
        )
      : await proposeBatch(calls, context, new Map(), IMPORT_PAGE)
  if (outcome.review)
    outcome.review.details = { ...outcome.review.details, rows }
  outcome.data = {
    ...(outcome.data && typeof outcome.data === "object" ? outcome.data : {}),
    rows: { ...rows, next: page.next },
    notIncludedCount: notIncluded.length,
  }
  return withNotIncluded(outcome, notIncluded)
}

/** Lines the model saw but could not read; display only, never turned into an action. */
export function notIncludedFromModel(value: unknown): NotIncluded[] {
  if (value === undefined) return []
  if (!Array.isArray(value))
    throw new VoiceError("List unclear lines as an array.")
  if (value.length > 20) throw new VoiceError("List at most 20 unclear lines.")
  return value.map((item) => {
    const row = (item ?? {}) as Record<string, unknown>
    const short = (v: unknown) => {
      if (v === undefined) return undefined
      if (typeof v !== "string" || v.length > 120)
        throw new VoiceError(
          "Describe each unclear line in at most 120 characters."
        )
      return v
    }
    const text = short(row.seen)
    if (!text) throw new VoiceError("Say what each unclear line shows.")
    const note = short(row.note)
    const line = Number.isInteger(row.line) ? (row.line as number) : undefined
    return {
      ...(line ? { line } : {}),
      text,
      reason: "unclear",
      ...(note ? { note } : {}),
    }
  })
}

/** Attach skipped rows to the card; the review steps stay exactly as signed. */
export function withNotIncluded(outcome: VoiceOutcome, rows: NotIncluded[]) {
  if (!rows.length) return outcome
  if (outcome.review)
    outcome.review.details = { ...outcome.review.details, notIncluded: rows }
  else
    outcome.data = {
      ...(outcome.data && typeof outcome.data === "object" ? outcome.data : {}),
      notIncluded: rows,
    }
  return outcome
}
