import "server-only"
import en from "@/messages/en.json"
import { rightFor } from "../routes"
import { VoiceError } from "./server"
import { voiceText } from "./capabilities.server"
import type { VoiceOutcome } from "./client-contract"

/** Fixed office pages the assistant may point at; text in `voice.pages.<id>`. Never a model-built path. */
export const PAGES: { id: string; route: string; record?: true }[] = [
  { id: "overview", route: "/" },
  { id: "products", route: "/products" },
  { id: "product", route: "/products", record: true },
  { id: "categories", route: "/products/categories" },
  { id: "stock", route: "/stock" },
  { id: "stock_negative", route: "/stock/negative" },
  { id: "orders", route: "/orders" },
  { id: "order", route: "/orders", record: true },
  { id: "invoices", route: "/invoices" },
  { id: "invoice", route: "/invoices", record: true },
  { id: "lots", route: "/lots" },
  { id: "lot", route: "/lots", record: true },
  { id: "supply", route: "/supply" },
  { id: "supply_links", route: "/supply/links" },
  { id: "supply_transfers", route: "/supply/transfers" },
  { id: "till_sales", route: "/till/sales" },
  { id: "till_shifts", route: "/till/shifts" },
  { id: "till_registers", route: "/till/registers" },
  { id: "till_users", route: "/till/users" },
  { id: "till_vaults", route: "/till/vaults" },
  { id: "invites", route: "/settings/invites" },
  { id: "scheduled_reports", route: "/settings/scheduled-reports" },
  { id: "app_keys", route: "/settings/app-keys" },
  { id: "webhooks", route: "/settings/webhooks" },
  { id: "search", route: "/search" },
]

export const PAGE_TOOL = "office_page_open"
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const pageText = (language?: string | null) =>
  voiceText(language).pages as Record<string, { name: string; purpose: string }>

/** Virtual read tool, like assistant_task_status: shows a page link; reads and changes nothing. */
export const pageTool = {
  id: PAGE_TOOL,
  summary:
    "Show the user a link to the Office page where they can do or see something. Say only the page name; the screen shows the link.",
  method: "GET",
  write: false,
  body: {
    type: "object",
    required: ["page"],
    properties: {
      page: {
        type: "string",
        enum: PAGES.map((p) => p.id),
        description: PAGES.map(
          (p) =>
            `${p.id}: ${(en.voice.pages as Record<string, { purpose: string }>)[p.id]!.purpose}`
        ).join("; "),
      },
      recordId: {
        type: "string",
        format: "uuid",
        description:
          "Only for product, order, invoice and lot: the record's id from a lookup.",
      },
    },
  },
}

/** Page questions ("take me to…", "where can I…") also get the page tool. */
export const asksForPage = (query: string) =>
  query === PAGE_TOOL ||
  /\b(page|screen|go to|take me|navigate|open|where)\b/i.test(query)

export function openPage(
  args: Record<string, unknown>,
  permissions: string[],
  language?: string | null
): VoiceOutcome {
  const page = PAGES.find((p) => p.id === args.page)
  if (!page) throw new VoiceError("This Office page is not available.")
  const id = args.recordId
  if (
    page.record ? !(typeof id === "string" && UUID.test(id)) : id !== undefined
  )
    throw new VoiceError("This Office page needs a valid record from a lookup.")
  const right = rightFor(page.route)
  if (
    right &&
    !permissions.some((p) =>
      [right, "workspace_admin", "workspace_owner"].includes(p)
    )
  )
    throw new VoiceError("You do not have permission for this page.", 403)
  const name = pageText(language)[page.id]!.name
  const href = page.record
    ? `${page.route}/${String(id).toLowerCase()}`
    : page.route
  return {
    status: "completed",
    data: { page: name },
    links: [{ label: name, href }],
    narration: name,
  }
}
