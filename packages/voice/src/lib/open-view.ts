/** What a host answers when the assistant asks to open a view (P4); never a write. */
export type OpenViewResult =
  | { status: "opened" | "already_open"; view: string; title: string }
  | { status: "not_found"; available: { view: string; title: string }[] }

/** The host's way to open a view (Flow passes one; Office opens none). */
export type OpenView = (view: string) => OpenViewResult

/** The `open_flow_view` client tool: navigation only, answered at once, never throws. */
export const openViewTool =
  (host?: OpenView) =>
  ({ view }: { view?: unknown }): string => {
    if (!host)
      return JSON.stringify({
        status: "unavailable",
        message: "Screens open by voice in Genesy Flow, not here.",
      })
    try {
      return JSON.stringify(host(typeof view === "string" ? view : ""))
    } catch {
      return JSON.stringify({ status: "failed" })
    }
  }
