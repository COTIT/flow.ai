/** The right each office route needs: the one the API checks on the list its page reads. */
const routeRights: [prefix: string, right: string][] = [
  ["/products", "product.list"],
  ["/stock/warehouses", "warehouse.list"],
  ["/stock/counts", "stocktake.list"],
  ["/stock/valuation", "accounting.read"],
  ["/stock", "stock.list"],
  ["/orders", "order.list"],
  ["/invoices", "invoice.list"],
  ["/payments", "payment.read"],
  ["/sales-returns", "salesreturn.list"],
  ["/purchases", "purchaseorder.list"],
  ["/supplier-bills", "supplierbill.list"],
  ["/supplier-payments", "supplierpayment.list"],
  ["/purchase-returns", "purchasereturn.list"],
  ["/supply/links", "workspace_admin"],
  ["/till", "cashregister.list"],
  ["/accounting", "accounting.read"],
  ["/settings/taxes", "taxprofile.read"],
  ["/settings/app-keys", "workspace_admin"],
  ["/settings/webhooks", "workspace_admin"],
  ["/settings/integrations", "integration.view"],
  ["/settings/storefront", "workspace_admin"],
  ["/settings/payments", "workspace_admin"],
  ["/settings/invites", "invite.list"],
  ["/settings/members", "workspace_admin"],
  ["/settings/imports", "import.list"],
  ["/search", "workspace.global-search"],
]

/** The right `path` needs, or undefined for a route every member may use (batches, transfers). */
export const rightFor = (path: string) =>
  routeRights.find(([p]) => path === p || path.startsWith(`${p}/`))?.[1]
