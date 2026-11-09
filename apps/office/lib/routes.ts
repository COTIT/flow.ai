/** The right each office route needs: the one the API checks on the list its page reads. */
const routeRights: [prefix: string, right: string][] = [
  ["/products", "product.list"],
  ["/stock", "stock.list"],
  ["/orders", "order.list"],
  ["/invoices", "invoice.list"],
  ["/supply/links", "workspace_admin"],
  ["/till", "cashregister.list"],
  ["/settings/app-keys", "workspace_admin"],
  ["/settings/webhooks", "workspace_admin"],
  ["/settings/invites", "invite.list"],
  ["/settings/members", "workspace_admin"],
  ["/search", "workspace.global-search"],
]

/** The right `path` needs, or undefined for a route every member may use (batches, transfers). */
export const rightFor = (path: string) =>
  routeRights.find(([p]) => path === p || path.startsWith(`${p}/`))?.[1]
