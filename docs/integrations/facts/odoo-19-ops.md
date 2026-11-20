# Odoo 19 operations facts pack (R2: R2-O1–R2-O11, for WF-02..WF-07)

Retrieved 2026-11-20. This pack holds facts only. Each fact cites an official source. Anything a source does not state is marked **UNVERIFIED**.

**Sources**
- **Source code:** `odoo/odoo` branch `19.0`, pinned at commit [`61d8ae93ae5d`](https://github.com/odoo/odoo/tree/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f), the same commit as the R1 pack. Every line link below points at that commit, so line numbers stay stable.
  - Short form used below: `GH/<path>#Lx` = `https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/<path>#Lx`.
- **Docs:** "External JSON-2 API", Odoo 19.0, <https://www.odoo.com/documentation/19.0/developer/reference/external_api.html> (called **[DOC]**). Odoo 20.0 version: <https://www.odoo.com/documentation/20.0/developer/reference/external_api.html> (called **[DOC20]**).
- **Functional docs:** "Control policies", <https://www.odoo.com/documentation/19.0/applications/inventory_and_mrp/purchase/manage_deals/control_bills.html> (**[DOC-BILLS]**). "Automation rules", <https://www.odoo.com/documentation/19.0/applications/studio/automated_actions.html> (**[DOC-AUTO]**). "Webhooks", <https://www.odoo.com/documentation/19.0/applications/studio/automated_actions/webhooks.html> (**[DOC-HOOKS]**).
  - **Caveat on doc pages:** the fetch tool returned the body of these pages as a summary, not verbatim text. Facts that rest **only** on a doc page are therefore marked **PARTIAL**. Facts read in the source code are not affected.

**What each verdict means** (same as `odoo-19.md`)
- **VERIFIED:** read directly in the cited source.
- **PARTIAL:** some of it is verified and the rest is listed as UNVERIFIED.
- **UNVERIFIED:** no official statement was found.

Nothing in this pack was run against a live Odoo instance. That is live gate L1.

**Transport:** every call below uses the JSON-2 transport in [`odoo-19.md` §0](odoo-19.md#0-json-2-transport-shared-by-every-operation-below), which this pack does not repeat. `search_read` / `read` arguments are in `odoo-19.md` O1.

**Module rule:** a field added by a module (for example `purchase_stock`, `sale_stock`) exists only when that module is installed. Check it with `fields_get` (R2-O9) before you request it. `odoo-19.md` O4 explains what happens when a requested field does not exist.

---

## R2-O1. Receipts: `stock.picking`, `stock.move`, `stock.move.line` (WF-02) — **VERIFIED (code)**

### `stock.picking.type.code` (how to tell a receipt from a delivery)

| Fact | Source |
|---|---|
| `code = Selection([('incoming','Receipt'),('outgoing','Delivery'),('internal','Internal Transfer')], default='incoming', required=True)` | [GH/addons/stock/models/stock_picking.py#L41](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L41) |
| `stock.picking.picking_type_code = Selection(related='picking_type_id.code')`. It is related and not stored | [stock_picking.py#L624-L626](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L624-L626) |

### `stock.picking`

| Field | Definition | Source |
|---|---|---|
| `state` | `draft`, `waiting` (Waiting Another Operation), `confirmed` (Waiting), `assigned` (Ready), `done`, `cancel`. `compute='_compute_state'`, `store=True`, `tracking=True` | [stock_picking.py#L574-L587](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L574-L587) |
| `date_done` | `Datetime('Date of Transfer')`. Help text: "Date at which the transfer has been processed or cancelled." | [stock_picking.py#L605](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L605) |
| `date_done` written | `_action_done` writes `{'date_done': fields.Datetime.now(), 'priority': '0'}` after moving the moves to done | [stock_picking.py#L1300-L1311](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L1300-L1311) |
| `scheduled_date` | Stored compute, indexed | [stock_picking.py#L594-L597](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L594-L597) |
| `origin` | `Char('Source Document')` | [stock_picking.py#L555-L557](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L555-L557) |
| `backorder_id`, `return_id` | Links to the backorder origin and to the returned picking | [stock_picking.py#L559-L566](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L559-L566) |
| `move_ids` | `One2many('stock.move','picking_id')` | [stock_picking.py#L616](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L616) |
| `partner_id`, `company_id` | `company_id` is related to `picking_type_id.company_id` and stored | [stock_picking.py#L630-L635](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_picking.py#L630-L635) |
| `purchase_id` (`purchase_stock`) | `Many2one('purchase.order', related='move_ids.purchase_line_id.order_id', readonly=True)`. Related, **not stored** | [GH/addons/purchase_stock/models/stock.py#L13-L15](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock.py#L13-L15) |

### `stock.move`

| Field | Definition | Source |
|---|---|---|
| `state` | `draft` (New), `waiting`, `confirmed`, `partially_available`, `assigned`, `done`, `cancel`. Not a compute | [GH/addons/stock/models/stock_move.py#L107-L120](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L107-L120) |
| `product_uom_qty` | 'Demand', in `product_uom` | [stock_move.py#L58-L65](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L58-L65) |
| `product_uom` | `Many2one('uom.uom', "Unit", required=True)` | [stock_move.py#L67-L70](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L67-L70) |
| `product_qty` | 'Real Quantity': the demand in the product's **default** UoM. `digits=0`, stored | [stock_move.py#L54-L57](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L54-L57) |
| `quantity` | Stored compute. Docstring: "the sum of the move lines `quantity`", in `product_uom` | [stock_move.py#L171-L172](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L171-L172), [#L421-L433](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L421-L433) |
| `date` | 'Date Scheduled'. Help: "Scheduled date until move is done, then date of actual move processing". `_action_done` writes `{'state': 'done', 'date': fields.Datetime.now()}` | [stock_move.py#L28-L30](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L28-L30), [#L2306](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L2306) |
| `location_id`, `location_dest_id`, `location_final_id` | Source, intermediate destination, and final destination of a move chain | [stock_move.py#L75-L90](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L75-L90) |
| `picking_id`, `origin`, `reference`, `is_inventory` | `reference` is a stored compute | [stock_move.py#L106](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L106), [#L130](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L130), [#L151](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L151), [#L183](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L183) |
| `purchase_line_id` (`purchase_stock`) | `Many2one('purchase.order.line', ondelete='set null', readonly=True)`, stored | [GH/addons/purchase_stock/models/stock_move.py#L15-L17](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock_move.py#L15-L17) |

### `stock.move.line` (the detailed operation: lot, package, actual quantity)

| Field | Definition | Source |
|---|---|---|
| `move_id`, `picking_id` | Links to the parent move and picking | [GH/addons/stock/models/stock_move_line.py#L22-L30](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L22-L30) |
| `product_uom_id` | `Many2one('uom.uom','Unit', required=True)`. Note the name: **`product_uom_id`** here, against **`product_uom`** on `stock.move` | [stock_move_line.py#L33-L36](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L33-L36) |
| `quantity` | In `product_uom_id`, stored | [stock_move_line.py#L38-L40](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L38-L40) |
| `quantity_product_uom` | 'Quantity in Product UoM', stored compute | [stock_move_line.py#L41-L43](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L41-L43) |
| `lot_id` | `Many2one('stock.lot')` | [stock_move_line.py#L49](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L49) |
| `date` | Help: "Creation date of this move line until updated due to: quantity being increased, 'picked' status has updated, or move line is done." | [stock_move_line.py#L61-L63](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L61-L63) |
| `state` | `related='move_id.state', store=True` | [stock_move_line.py#L84](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L84) |
| `reference`, `origin` | Related to the move | [stock_move_line.py#L90-L92](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L90-L92) |

---

## R2-O2. Purchase order and line: received and billed quantities (WF-02) — **VERIFIED (code)**

### `purchase.order.line`

| Field | Definition | Source |
|---|---|---|
| `qty_received_method` | Base selection is `[('manual','Manual')]`. `purchase_stock` adds `('stock_moves','Stock Moves')` | [GH/addons/purchase/models/purchase_order_line.py#L68-L71](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L68-L71); [GH/addons/purchase_stock/models/purchase_order_line.py#L25](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order_line.py#L25) |
| Method rule | Base: product `type` in (`consu`,`service`) gives `manual`. With `purchase_stock`: `type == 'consu'` gives `stock_moves` (non-display lines) | [purchase_order_line.py#L226-L231](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L226-L231); [purchase_stock/…/purchase_order_line.py#L37-L41](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order_line.py#L37-L41) |
| `qty_received` | `Float("Received Qty", compute, inverse, compute_sudo=True, store=True, digits='Product Unit')`, in the line's `product_uom_id` | [purchase_order_line.py#L72](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L72) |
| `qty_received` with `stock_moves` | The sum over the line's moves with `state == 'done'`, converted to `line.product_uom_id` (`rounding_method='HALF-UP'`). Purchase returns are subtracted (when `to_refund` or no origin). Dropship-return and `_should_count_for_quantity_received()` edge cases are skipped | [purchase_stock/…/purchase_order_line.py#L55-L80](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order_line.py#L55-L80) |
| `qty_received` with `manual` | `qty_received_manual` | [purchase_order_line.py#L251-L258](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L251-L258) |
| `invoice_lines` | `One2many('account.move.line','purchase_line_id', string="Bill Lines")` | [purchase_order_line.py#L63](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L63) |
| `qty_invoiced` | `Float("Billed Qty", compute='_compute_qty_invoiced', store=True)` | [purchase_order_line.py#L66](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L66) |
| `qty_invoiced` rule | The sum over `invoice_lines` whose bill `state` is **not `cancel`** (or whose `payment_state == 'invoicing_legacy'`). `in_invoice` adds and `in_refund` subtracts, converted to the line UoM with `round=False`. **Draft bills count.** | [purchase_order_line.py#L197-L207](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L197-L207) |
| `qty_to_invoice` | Stored. If the order `state == 'purchase'`: when `product_id.purchase_method == 'purchase'` it is `product_qty - qty_invoiced`; otherwise it is `qty_received - qty_invoiced`. In any other order state it is `0` | [purchase_order_line.py#L74-L75](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L74-L75), [#L171-L184](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L171-L184) |
| `move_ids` (`purchase_stock`) | `One2many('stock.move','purchase_line_id')` | [purchase_stock/…/purchase_order_line.py#L28](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order_line.py#L28) |

### `purchase.order`

| Field | Definition | Source |
|---|---|---|
| `state` | `draft` (RFQ), `sent`, `to approve`, `purchase`, `cancel`. There is no `done` state in 19.0 | [GH/addons/purchase/models/purchase_order.py#L105-L111](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L105-L111) |
| `invoice_status` | `no` (Nothing to Bill), `to invoice` (Waiting Bills), `invoiced` (Fully Billed). Stored | [purchase_order.py#L127-L130](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L127-L130) |
| `invoice_status` rule | State not `purchase` gives `no`. Any non-display line with `qty_to_invoice ≠ 0` (at "Product Unit" precision) gives `to invoice`. All zero **and** `invoice_ids` present gives `invoiced`. Otherwise `no` | [purchase_order.py#L46-L68](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L46-L68) |
| `invoice_ids`, `invoice_count` | Stored compute = `order_line.invoice_lines.move_id` | [purchase_order.py#L70-L75](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L70-L75), [#L125-L126](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L125-L126) |
| `picking_ids` (`purchase_stock`) | Stored compute = `order_line.move_ids.picking_id` | [GH/addons/purchase_stock/models/purchase_order.py#L22](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L22), [#L44-L47](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L44-L47) |
| `receipt_status` (`purchase_stock`) | `pending`, `partial`, `full`, or `False`. No pickings (or all cancelled) gives `False`. All done/cancel gives `full`. Any done gives `partial`. Otherwise `pending` | [purchase_stock/…/purchase_order.py#L35-L39](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L35-L39), [#L69-L78](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L69-L78) |

### Product control policy (drives `qty_to_invoice`)

| Fact | Source |
|---|---|
| `product.template.purchase_method = Selection([('purchase','On ordered quantities'),('receive','On received quantities')], string="Control Policy", store=True)` | [GH/addons/purchase/models/product.py#L14-L19](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/product.py#L14-L19) |
| Compute: `service` gives `purchase`. Any other type gives `default_get(['purchase_method'])`, falling back to `'receive'` | [product.py#L22-L29](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/product.py#L22-L29) |

---

## R2-O3. Vendor bills: `account.move` / `account.move.line` (WF-02) — **VERIFIED (code)**

| Field | Definition | Source |
|---|---|---|
| `move_type` | `entry`, `out_invoice`, `out_refund`, `in_invoice` (Vendor Bill), `in_refund` (Vendor Credit Note), `out_receipt`, `in_receipt`. Indexed | [GH/addons/account/models/account_move.py#L143-L160](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L143-L160) |
| `state` | `draft`, `posted`, `cancel` | [account_move.py#L130-L142](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L130-L142) |
| `payment_state` | Stored compute. Values: `not_paid`, `in_payment`, `paid`, `partial`, `reversed`, `blocked`, `invoicing_legacy` | [account_move.py#L49-L57](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L49-L57), [#L606-L612](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L606-L612) |
| `payment_state` rule | A `blocked` or `invoicing_legacy` value is kept as it is. An invoice qualifies for payment computation when `posted`, or `draft` with a non-zero total. Anything else gets `not_paid` | [account_move.py#L1248-L1264](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L1248-L1264) |
| `account.move.line.purchase_line_id` | `Many2one('purchase.order.line', ondelete='set null', copy=False)`, stored, indexed | [GH/addons/purchase/models/account_invoice.py#L525-L531](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/account_invoice.py#L525-L531) |
| `account.move.line.purchase_order_id` | `related='purchase_line_id.order_id'`, **not stored** | [account_invoice.py#L531](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/account_invoice.py#L531) |
| `account.move.purchase_id` | `store=False`. It is a form helper used to fill lines from a PO, **not a stored link** | [account_invoice.py#L22-L24](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/account_invoice.py#L22-L24) |

- **Consequence:** the reliable bill→PO link is `account.move.line.purchase_line_id`, or reading `purchase.order.invoice_ids`. Filter bill lines with `[('purchase_line_id.order_id','=',po_id)]`.

---

## R2-O4. Native bill controls and 3-way match (WF-02) — **PARTIAL**

**What Odoo Community 19.0 has in source (VERIFIED):**

| Control | Fact | Source |
|---|---|---|
| Control policy | `purchase_method` decides whether `qty_to_invoice` follows ordered or received quantity (R2-O2) | [GH/addons/purchase/models/product.py#L14-L29](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/product.py#L14-L29) |
| 3-way matching setting | `res.config.settings.module_account_3way_match = Boolean("3-way matching: purchases, receptions and bills")`. A `module_*` setting installs module `account_3way_match` | [GH/addons/purchase/models/res_config_settings.py#L17](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/res_config_settings.py#L17) |
| Module location | `addons/account_3way_match/__manifest__.py` returns **404** in `odoo/odoo` at the pinned commit, so the module is **not** in the Community repository | GitHub raw fetch, 404 (2026-11-20) |
| Payment block | `account.move.action_toggle_block_payment()` sets `payment_state = 'blocked'`. It refuses when the state is already `paid` or `in_payment` ("You can't block a paid invoice."), and toggles back to `not_paid` plus recompute | [GH/addons/account/models/account_move.py#L6455-L6464](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L6455-L6464) |
| What the block stops | `action_force_register_payment` raises `UserError("You cannot register payments for blocked invoices.")`. `action_register_payment` calls it for posted moves | [account_move.py#L6155-L6165](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L6155-L6165) |
| Remote call | `action_toggle_block_payment` has no `_` prefix and no `@api.private`, so JSON-2 can call it (with `ids`) | rule at [GH/odoo/service/model.py#L45-L71](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/service/model.py#L45-L71) |
| Manual matching view | `purchase.bill.line.match` (`_auto = False`, a SQL view) puts PO lines and bill lines side by side (`pol_id`, `aml_id`, `qty_invoiced`, `qty_to_invoice`, `line_amount_untaxed`, `state`). Actions: `action_match_lines`, `action_add_to_po` | [GH/addons/purchase/models/purchase_bill_line_match.py#L10-L35](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_bill_line_match.py#L10-L35), [#L163](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_bill_line_match.py#L163), [#L202](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_bill_line_match.py#L202) |

**What the docs say about 3-way matching (PARTIAL: [DOC-BILLS], from a summary):**
- Enable it at Purchase › Configuration › Settings › Invoicing › "3-way matching".
- It adds a **"Should Be Paid"** field on the vendor bill (Other Info tab), with values **Yes / No / Exception**:
  - bills created from a PO start at Yes;
  - editing a draft bill sets Exception;
  - No is set after payment.
- The page says Exception "does not block the changes or display an error message".
- **UNVERIFIED:**
  - the technical field name of "Should Be Paid" and its selection keys;
  - whether it blocks payment registration;
  - the edition it needs (the module is absent from Community source, so it is presumably Enterprise, but no source states this).

---

## R2-O5. Stock levels and recent movements (WF-04) — **VERIFIED (code)**

### `stock.quant`

| Field | Definition | Source |
|---|---|---|
| `location_id` | Required, indexed | [GH/addons/stock/models/stock_quant.py#L57-L60](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L57-L60) |
| `warehouse_id` | `related='location_id.warehouse_id'` | [stock_quant.py#L61](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L61) |
| `lot_id`, `package_id`, `owner_id` | Quant dimensions | [stock_quant.py#L64-L77](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L64-L77) |
| `quantity` | Help: "in the default unit of measure of the product". `readonly` | [stock_quant.py#L78-L81](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L78-L81) |
| `reserved_quantity` | Default UoM, `readonly`, `required` | [stock_quant.py#L82-L86](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L82-L86) |
| `available_quantity` | **Exists.** Compute, **not stored**: `quantity - reserved_quantity` | [stock_quant.py#L87-L90](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L87-L90), [#L120-L122](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L120-L122) |
| `in_date` | Incoming date, required | [stock_quant.py#L91](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_quant.py#L91) |

### `stock.location` and `stock.warehouse`

| Fact | Source |
|---|---|
| `usage` values: `supplier`, `view`, `internal`, `customer`, `inventory` (Inventory Loss), `production`, `transit`. Default `internal`. Help: "Internal: Physical locations inside your warehouses" | [GH/addons/stock/models/stock_location.py#L32-L46](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_location.py#L32-L46) |
| `stock.location.warehouse_id`: a stored compute. It is the warehouse whose `view_location_id` is a parent of the location | [stock_location.py#L85](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_location.py#L85), [#L160-L168](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_location.py#L160-L168) |
| `stock.warehouse`: `view_location_id` (usage `view`), `lot_stock_id` (usage `internal`, the main stock location), `code` (5 characters), `in_type_id`, `out_type_id` | [GH/addons/stock/models/stock_warehouse.py#L42-L50](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_warehouse.py#L42-L50), [#L75-L76](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_warehouse.py#L75-L76) |

### `product.product` quantity fields (all compute, **not stored**, `compute_sudo=False`)

Fields at [GH/addons/stock/models/product.py#L51-L109](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L51-L109). The compute is `_compute_quantities` → `_compute_quantities_dict`, [product.py#L145-L274](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L145-L274).

| Field | Formula (from `_compute_quantities_dict`) | Source |
|---|---|---|
| `qty_available` | Σ `stock.quant.quantity` over the location domain. If `to_date` is in the past, done move lines after `to_date` are subtracted or added back | [product.py#L230-L231](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L230-L231), [#L261-L264](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L261-L264) |
| `free_qty` | `qty_available − Σ reserved_quantity − expired unreserved` (expiry only with the `with_expiration` context) | [product.py#L268](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L268) |
| `incoming_qty` / `outgoing_qty` | Σ `stock.move.product_qty` for moves in state `waiting`, `confirmed`, `assigned`, `partially_available` that enter or leave the locations. Optional `from_date`/`to_date` filter on `stock.move.date` | [product.py#L216-L229](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L216-L229), [#L269-L270](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L269-L270) |
| `virtual_available` | `qty_available + incoming_qty − outgoing_qty − expired unreserved` | [product.py#L271-L273](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L271-L273) |
| Rounding | Each value goes through `product.uom_id.round(...)`, in the product's default UoM | [product.py#L267-L273](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L267-L273) |
| Services | `type == 'service'` gives 0 for all | [product.py#L151-L161](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L151-L161) |
| Context keys | `lot_id`, `owner_id`, `package_id`, `from_date`, `to_date`, `location`, `warehouse_id` (`@api.depends_context`) | [product.py#L145-L150](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L145-L150) |
| Location scope | `warehouse_id` context → that warehouse's `view_location_id` subtree. `location` context → that location subtree (`strict` → exactly those ids). **No context → the `view_location_id` of every warehouse of `env.companies`** (all descendants via recursive SQL). The field help says "any Stock Location with 'internal' type"; the code uses warehouse view locations | [product.py#L350-L403](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L350-L403), [#L405-L445](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/product.py#L405-L445) |

- **Over JSON-2:** send the scope in `context`, for example `{"context":{"warehouse_id": 1}}` on `product.product/read` or `search_read`.
  - That the JSON-2 `context` reaches `self.env.context` is stated in [DOC] ("context" key) and covered in `odoo-19.md` §0.
  - The exact effect has not been checked live (L1).

### "Recent movements"

- Use `stock.move.line` (or `stock.move`) with `state='done'`, ordered by `date desc`.
- On done moves and done move lines, `date` is the processing time (R2-O1).
- `location_usage` and `location_dest_usage` are related fields on both models ([stock_move.py#L91-L92](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move.py#L91-L92), [stock_move_line.py#L74-L75](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L74-L75)).
- `is_inventory` marks inventory adjustments ([stock_move_line.py#L86](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_move_line.py#L86)).

---

## R2-O6. Sales: `sale.order`, deliveries, customer invoices (WF-05) — **VERIFIED (code)**

| Fact | Source |
|---|---|
| `sale.order.state`: `draft` (Quotation), `sent`, `sale` (Sales Order), `cancel`. There is no `done` state in 19.0 | [GH/addons/sale/models/sale_order.py#L26-L31](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L26-L31), [#L71-L76](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L71-L76) |
| `invoice_status`: `upselling`, `invoiced`, `to invoice`, `no`. Stored compute | [sale_order.py#L19-L24](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L19-L24), [#L246-L250](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L246-L250) |
| `invoice_status` rule: state not `sale` gives `no`. Otherwise it aggregates the line `invoice_status` (lines that are not down-payment and not display) | [sale_order.py#L620-L650](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L620-L650) |
| SO → invoices: `invoice_ids` is a compute, **not stored**, **with `search='_search_invoice_ids'`** (so it can be used in a domain). Value = `order_line.invoice_lines.move_id` filtered to `out_invoice`, `out_refund`, `out_receipt` | [sale_order.py#L240-L244](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L240-L244), [#L571-L579](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order.py#L571-L579) |
| Line link: `sale.order.line.invoice_lines` ↔ `account.move.line.sale_line_ids`, Many2many through table `sale_order_line_invoice_rel` | [GH/addons/sale/models/sale_order_line.py#L257-L261](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order_line.py#L257-L261); [GH/addons/sale/models/account_move_line.py#L12-L16](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/account_move_line.py#L12-L16) |
| Line quantities: `qty_delivered_method`, `qty_delivered`, `qty_invoiced`, `qty_to_invoice` exist on `sale.order.line` | [sale_order_line.py#L217-L250](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/models/sale_order_line.py#L217-L250) |
| SO → deliveries (`sale_stock`): `picking_ids = One2many('stock.picking','sale_id')`. `stock.picking.sale_id` is a **stored** compute = the first `move_ids.sale_line_id.order_id` | [GH/addons/sale_stock/models/sale_order.py#L31](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/sale_order.py#L31); [GH/addons/sale_stock/models/stock.py#L187-L195](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/stock.py#L187-L195) |
| `stock.move.sale_line_id` (`sale_stock`) | [sale_stock/…/stock.py#L17](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/stock.py#L17) |
| `delivery_status`: `pending`, `started`, `partial`, `full`, or `False`. Stored compute from picking states and line `qty_delivered` | [sale_stock/…/sale_order.py#L33-L38](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/sale_order.py#L33-L38), [#L90-L107](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/sale_order.py#L90-L107) |
| `effective_date`: stored = min `date_done` over pickings with `state == 'done'` and `location_dest_id.usage == 'customer'` | [sale_stock/…/sale_order.py#L51](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/sale_order.py#L51), [#L83-L88](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/models/sale_order.py#L83-L88) |
| A delivery is a picking with `picking_type_code == 'outgoing'`. "Fulfilled" = `state == 'done'`, with `date_done` as the time | R2-O1 |
| Customer invoice: `account.move` with `move_type == 'out_invoice'` (credit note `out_refund`). `state`, `payment_state` as in R2-O3 | [account_move.py#L143-L160](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_move.py#L143-L160) |

---

## R2-O7. Aggregation and date/time over JSON-2 (WF-05) — **VERIFIED (code)**; [DOC] is silent on aggregation

JSON-2 can call a method only if it is public: no `_` prefix and no `@api.private` anywhere in the MRO ([GH/odoo/service/model.py#L45-L71](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/service/model.py#L45-L71)). `@api.deprecated` is Python's `warnings.deprecated`: it only warns and does **not** block remote calls ([GH/odoo/orm/decorators.py#L12-L50](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/decorators.py#L12-L50)).

| Method | Signature (19.0) | Decorators | JSON-2 callable? | Source |
|---|---|---|---|---|
| `formatted_read_group` | `(domain, groupby=(), aggregates=(), having=(), offset=0, limit=None, order=None)`. `groupby` takes `field` or `field:granularity` (`day`/`week`/`month`/`quarter`/`year`, plus `*_number` parts). `aggregates` takes `field:agg` | `@api.model`, `@api.readonly` | **Yes** (module `web`, `auto_install: True`) | [GH/addons/web/models/models.py#L800-L850](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/web/models/models.py#L800-L850); [GH/addons/web/__manifest__.py#L14-L15](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/web/__manifest__.py#L14-L15) |
| `formatted_read_grouping_sets` | — | `@api.model`, `@api.readonly` | Yes | [web/models/models.py#L700-L702](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/web/models/models.py#L700-L702) |
| `web_read_group` | — | `@api.model`, `@api.readonly` | Yes | [web/models/models.py#L347-L349](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/web/models/models.py#L347-L349) |
| `read_group` | `(domain, fields, groupby, offset=0, limit=None, orderby=False, lazy=True)` | `@api.model`, `@api.readonly`, `@api.deprecated("Since 19.0 … use … formatted_read_group …")` | Yes, but deprecated | [GH/odoo/orm/models.py#L2752-L2755](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L2752-L2755) |
| `_read_group` | — | `_` prefix | **No** (403) | [models.py#L1867](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L1867) |
| `search_count` | `(domain, limit=None)` | `@api.model`, `@api.readonly` | Yes | `odoo-19.md` O1 |

**Dates and time zones**

| Fact | Source |
|---|---|
| `Datetime` column type is `timestamp` (without time zone). `to_datetime` raises `ValueError("Datetime field expects a naive datetime")` for tz-aware values. `context_timestamp` documents stored values as "naive datetime value (expressed in UTC)" | [GH/odoo/orm/fields_temporal.py#L191-L194](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L191-L194), [#L211-L227](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L211-L227), [#L232-L246](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L232-L246) |
| JSON output is `"%Y-%m-%d %H:%M:%S"`, UTC, whole seconds | `odoo-19.md` O1 |
| `Date` fields (for example `account.move.invoice_date`, `date`) are plain `date` columns with no time zone | [fields_temporal.py#L106-L109](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L106-L109) |
| **Grouping by a datetime granularity uses `context['tz']`**: SQL `timezone(tz, timezone('UTC', col))` when `tz` is a known pytz zone. Otherwise it logs a warning and groups in UTC. Domains on datetimes compare raw UTC values | [fields_temporal.py#L80-L87](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L80-L87) |

- **Money note (project rule):** Odoo aggregates are inputs only. Verzo's own totals stay in Postgres `numeric` on our side.

---

## R2-O8. Replenishment: orderpoints, vendor prices, open RFQs (WF-06) — **VERIFIED (code)**

### `stock.warehouse.orderpoint`

| Field | Definition | Source |
|---|---|---|
| `trigger` | `auto` / `manual`, default `auto` | [GH/addons/stock/models/stock_orderpoint.py#L31-L32](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L31-L32) |
| `product_min_qty`, `product_max_qty` | "Min Quantity" (triggers replenishment). "Max Quantity" (level to reach) | [stock_orderpoint.py#L56-L62](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L56-L62) |
| `route_id` | `Many2one('stock.route')`. `effective_route_id` is a non-stored compute | [stock_orderpoint.py#L76-L84](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L76-L84) |
| `qty_on_hand`, `qty_forecast` | Non-stored compute. `qty_forecast = virtual_available` (at the location, to the lead-horizon date) `+ _quantity_in_progress()` | [stock_orderpoint.py#L85-L86](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L85-L86), [#L376-L396](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L376-L396), [#L489-L495](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L489-L495) |
| `qty_to_order` | Non-stored, with a search method. It is `qty_to_order_manual` if set, else `qty_to_order_computed` (stored) | [stock_orderpoint.py#L87-L89](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L87-L89), [#L397-L400](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L397-L400) |
| Computed quantity | If `qty_forecast < product_min_qty`: `max(min,max) − (virtual_available + in_progress)`, rounded up to the `replenishment_uom_id` multiple. Otherwise 0 | [stock_orderpoint.py#L465-L480](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L465-L480) |
| `supplier_id` (`purchase_stock`) | Vendor pinned on the orderpoint | [GH/addons/purchase_stock/models/stock.py#L145](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock.py#L145) |

### Open RFQ quantity (the "in progress" count)

- `purchase_stock` overrides `_quantity_in_progress` with the sum of `purchase.order.line.product_qty` for lines in orders whose state is **`draft`, `sent` or `to approve`**, matched by orderpoint location, line final location, or the PO's picking-type destination. It is converted to the product UoM ([GH/addons/purchase_stock/models/stock.py#L322-L329](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock.py#L322-L329); [GH/addons/purchase_stock/models/product.py#L165-L193](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/product.py#L165-L193)).
- **Confirmed** POs (`purchase`) already count through their incoming moves in `virtual_available` (R2-O5).
- So Odoo's `qty_to_order` already removes both open RFQs and confirmed POs at that location.

### `action_replenish`: **it creates or updates RFQs directly**

| Step | Fact | Source |
|---|---|---|
| Entry | `action_replenish(self, force_to_max=False)`: public, record method, so JSON-2 can call it. `action_replenish_auto` sets `trigger='auto'` and then calls it | [stock_orderpoint.py#L346-L374](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L346-L374) |
| Procure | `_procure_orderpoint_confirm` builds one `Procurement` per orderpoint where `qty_to_order > 0` and runs `stock.rule.run(...)` with `from_orderpoint=True` | [stock_orderpoint.py#L696-L735](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_orderpoint.py#L696-L735) |
| Buy rule | `_run_buy` looks up an existing PO with `_make_po_get_domain`: same partner, `state='draft'`, same `picking_type_id`, company, `user_id = partner.buyer_id`, currency, plus a grouping by `group_rfq`. **If one is found, it appends or merges lines into it. Otherwise it creates a new `purchase.order` as SUPERUSER.** The PO stays in `draft` (RFQ) | [GH/addons/purchase_stock/models/stock_rule.py#L95-L165](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock_rule.py#L95-L165), [#L358-L365](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock_rule.py#L358-L365) |
| No vendor | It raises `ProcurementException` "There is no matching vendor price to generate the purchase order …" | [stock_rule.py#L59-L91](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock_rule.py#L59-L91) |
| Vendor choice | `values['supplierinfo_id']`, then `orderpoint.supplier_id`, then `product._select_seller(partner, quantity, date, uom)`, then the first seller with no price check | [stock_rule.py#L174-L199](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/stock_rule.py#L174-L199) |
| Scheduler | `stock.rule._run_scheduler_tasks` searches orderpoints with **`trigger='auto'`** and an active product, then calls `_procure_orderpoint_confirm`. So **auto orderpoints create RFQs on Odoo's own schedule**, without any Verzo call | [GH/addons/stock/models/stock_rule.py#L694-L708](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_rule.py#L694-L708), [#L747-L752](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_rule.py#L747-L752) |

- **Consequence for WF-06** (derived from the code above): if a customer has `auto` orderpoints, Odoo already drafts RFQs.
  - Verzo creating its own PO for the same product and location would double-order, unless it reads open RFQs first (`_quantity_in_progress` does this inside Odoo).
  - Calling `action_replenish` instead of creating a PO lets Odoo dedupe into the existing draft RFQ.
  - Which cron calls `_run_scheduler_tasks`, and how often, is **UNVERIFIED** (data files not read).

### `product.supplierinfo` (module `product`)

| Field | Definition | Source |
|---|---|---|
| `partner_id` | Vendor | [GH/addons/product/models/product_supplierinfo.py#L13](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L13) |
| `min_qty` | Help: "The quantity to purchase from this vendor to benefit from the unit price", in the vendor UoM if one is set | [product_supplierinfo.py#L27-L29](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L27-L29) |
| `price` | `Float('Unit Price', min_display_digits='Product Price')`: not rounded on storage (see `odoo-19.md` O6) | [product_supplierinfo.py#L30-L31](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L30-L31) |
| `currency_id`, `date_start`, `date_end`, `product_id`, `product_tmpl_id`, `product_uom_id`, `discount` | Price validity and scope | [product_supplierinfo.py#L25](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L25), [#L36-L50](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L36-L50), [#L54](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L54) |
| `delay` | `Integer('Lead Time', default=1)`. Help: "Lead time in days between the confirmation of the purchase order and the receipt of the products" | [product_supplierinfo.py#L51-L53](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/models/product_supplierinfo.py#L51-L53) |

---

## R2-O9. Change detection and capability discovery (WF-03 / WF-07) — **PARTIAL**

### Remote-callable check (rule in R2-O7)

| Method | Decorators | JSON-2 callable? | What it does | Source |
|---|---|---|---|---|
| `fields_get(allfields=None, attributes=None)` | `@api.model` | **Yes** | Leaves out fields the user cannot read (`_has_field_access(field,'read')`, driven by field `groups`). Marks a field `readonly` if not writable | [GH/odoo/orm/models.py#L3348-L3370](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L3348-L3370) |
| `has_access(operation)` | none (record method) | **Yes** | Returns bool. With empty `ids` it checks the model-level right | [models.py#L4105-L4128](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L4105-L4128) |
| `check_access(operation)` | `@api.private  # use has_access` | **No** (403) | Raises on denial | [models.py#L4105-L4106](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L4105-L4106) |
| `check_access_rights(operation, raise_exception=True)` | `@api.model`, `@api.deprecated("… since 18.0; use check_access()")` | Yes (deprecated) | — | [models.py#L4166-L4168](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L4166-L4168) |
| `check_field_access_rights` | `@api.model`, `@api.deprecated("… use `_check_field_access` … use `fields_get`")` | Yes (deprecated) | — | [models.py#L3436-L3442](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L3436-L3442) |
| `get_metadata()` | none | Yes | Record metadata | [models.py#L3950](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L3950) |
| `stock.rule.run_scheduler` | `@api.model` | Yes (public) | Runs procurement scheduling | [GH/addons/stock/models/stock_rule.py#L735-L736](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/models/stock_rule.py#L735-L736) |

### Discovery models and their read rights

| Model | Read access | Source |
|---|---|---|
| `ir.module.module` (installed modules) | **`base.group_system` only** (1,1,1,1). A least-privilege bot **cannot** list installed modules. Use `fields_get` on the target models instead | [GH/odoo/addons/base/security/ir.model.access.csv#L25](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L25) |
| `ir.model` | `group_erp_manager` full; `base.group_user` **0,0,0,0** | [ir.model.access.csv#L10](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L10), [#L18](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L18) |
| `mail.message` | `base.group_user` 1,1,1,1; `group_portal` 1,1,1,1; `group_public` read | [GH/addons/mail/security/ir.model.access.csv#L3-L5](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/mail/security/ir.model.access.csv#L3-L5) |
| `mail.message.tracking_value_ids` | Field `groups="base.group_system"` | [GH/addons/mail/models/mail_message.py#L180-L183](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/mail/models/mail_message.py#L180-L183) |
| `mail.tracking.value` | ACL: `base.group_system` only. Docstring: "this model is admin-only". Non-admins see a tracking value only through `_filter_has_field_access` | [mail/security/ir.model.access.csv#L37](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/mail/security/ir.model.access.csv#L37); [GH/addons/mail/models/mail_tracking_value.py#L37-L43](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/mail/models/mail_tracking_value.py#L37-L43) |

- **Change detection without polling:**
  - A non-admin bot can poll `mail.message` with `model`, `res_id` and `date` ([mail_message.py#L92](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/mail/models/mail_message.py#L92), [#L113-L114](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/mail/models/mail_message.py#L113-L114)).
  - It **cannot** read the tracked old and new values without `base.group_system`.
  - `stock.picking` (`_inherit mail.thread`, `state` tracked) and `purchase.order` / `sale.order` (state `tracking=True`) post tracking messages.
  - The record-level rule for reading `mail.message` was not traced: **UNVERIFIED**.
  - `write_date` polling (`odoo-19.md` O1) stays the primary method.

### Webhooks: automation rules (`base_automation`) and server action `webhook`

| Fact | Source |
|---|---|
| **Outgoing:** `ir.actions.server.state` includes `('webhook','Send Webhook Notification')`. Fields: `webhook_url` and `webhook_field_ids` | [GH/odoo/addons/base/models/ir_actions.py#L617](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/ir_actions.py#L617), [#L703-L709](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/ir_actions.py#L703-L709) |
| **Outgoing payload:** JSON object `{"_model": <model>, "_id": <record id>, "_action": "<action name>(#<action id>)"}` plus `record.read(webhook_field_ids, load=None)`, serialized with `json.dumps(sort_keys=True, default=str)`. The field help text says `'_name'`, but the **code sends `'_action'`** | [ir_actions.py#L1041-L1062](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/ir_actions.py#L1041-L1062) |
| **Delivery:** sent **after commit** (`cr.postcommit`), cancelled on rollback. `requests.post(url, data=json, headers={'Content-Type':'application/json'}, timeout=1)`. **No signature, no auth header, no retry.** Failures and timeouts are only logged ("send and forget") | [ir_actions.py#L1064-L1084](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/ir_actions.py#L1064-L1084) |
| Group-restricted fields in the payload give a warning ("Group-restricted fields cannot be included in webhook payloads…") | [ir_actions.py#L784-L797](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/ir_actions.py#L784-L797) |
| **Triggers** (`base.automation.trigger`): `on_create`, `on_create_or_write`, `on_write` (deprecated), `on_state_set`, `on_unlink`, `on_time*`, `on_message_*`, `on_webhook`, and others | [GH/addons/base_automation/models/base_automation.py#L173-L197](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/base_automation/models/base_automation.py#L173-L197) |
| **Incoming:** an `on_webhook` rule exposes `url = "<base_url>/web/hook/<webhook_uuid>"`. `action_rotate_webhook_uuid` regenerates it. `log_webhook_calls` is optional | [base_automation.py#L160-L164](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/base_automation/models/base_automation.py#L160-L164), [#L288-L294](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/base_automation/models/base_automation.py#L288-L294), [#L561-L563](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/base_automation/models/base_automation.py#L561-L563) |
| Who can configure automation: `base.automation` ACL `base.group_system` only | [GH/addons/base_automation/security/ir.model.access.csv#L2](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/base_automation/security/ir.model.access.csv#L2) |
| Docs (PARTIAL, summary): [DOC-AUTO] describes "Send Webhook Notification" as a POST with the selected fields. [DOC-HOOKS] says the incoming URL "is confidential". Neither states a signature, retries or limits. [DOC] does not mention webhooks | [DOC-AUTO], [DOC-HOOKS], [DOC] |

- **Consequence** (derived from the code above): outgoing webhooks are **hints only**.
  - Delivery is not guaranteed: 1 s timeout and no retry.
  - The sender is not authenticated.
  - A webhook can trigger an early poll but cannot replace the `write_date` safety net.

---

## R2-O10. Least-privilege read access per model — **PARTIAL** (ACL lines VERIFIED; combined effect with record rules UNVERIFIED)

Read = `perm_read=1` in `ir.model.access.csv`. Record rules then narrow the rows.

| Model | Lowest groups with read (file#line) | Record rules to know |
|---|---|---|
| `purchase.order`, `purchase.order.line` | `purchase.group_purchase_user` ([purchase csv#L2](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L2), [#L7](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L7)); `account.group_account_readonly` read-only ([#L4](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L4), [#L9](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L9)); `stock.group_stock_user` read-only via `purchase_stock` ([purchase_stock csv#L2-L3](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/security/ir.model.access.csv#L2-L3)) | Company rule `company_id in company_ids` ([purchase_security.xml#L42-L49](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/purchase_security.xml#L42-L49)) |
| `purchase.bill.line.match` | `purchase.group_purchase_user`, `account.group_account_readonly` ([purchase csv#L11-L12](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L11-L12)) | — |
| `account.move`, `account.move.line` | `account.group_account_readonly` ([account csv#L37](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/security/ir.model.access.csv#L37), [#L39](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/security/ir.model.access.csv#L39)); `purchase.group_purchase_user` ([purchase csv#L25](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L25), [#L27](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/ir.model.access.csv#L27)); `sales_team.group_sale_salesman` read ([sale csv#L8-L9](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/security/ir.model.access.csv#L8-L9)) | Company rule ([account_security.xml#L128-L137](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/security/account_security.xml#L128-L137)). **`purchase.group_purchase_user` rule: only `move_type in (in_invoice, in_refund, in_receipt)`** ([purchase_security.xml#L63-L73](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/security/purchase_security.xml#L63-L73)). `group_account_invoice` has a see-all rule ([account_security.xml#L232-L243](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/security/account_security.xml#L232-L243)) |
| `sale.order`, `sale.order.line` | `account.group_account_readonly` read-only ([sale csv#L29](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/security/ir.model.access.csv#L29), [#L36](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/security/ir.model.access.csv#L36)); `sales_team.group_sale_salesman` ([#L32](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/security/ir.model.access.csv#L32), [#L39](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/security/ir.model.access.csv#L39)) | Company rule. **`group_sale_salesman` sees only own or unassigned orders** (`user_id = user` or empty); `group_sale_salesman_all_leads` sees all ([sale ir_rules.xml#L44-L54](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale/security/ir_rules.xml#L44-L54)) |
| `stock.picking` | `stock.group_stock_user` ([stock csv#L7](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L7)); `account.group_account_readonly` read-only via `stock_account` ([stock_account csv#L5](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock_account/security/ir.model.access.csv#L5)); `purchase.group_purchase_user` ([purchase_stock csv#L6](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/security/ir.model.access.csv#L6)); `sales_team.group_sale_salesman` ([sale_stock csv#L2](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/security/ir.model.access.csv#L2)). **`base.group_user` has no read** | Company rules ([stock_security.xml#L75-L165](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/stock_security.xml#L75-L165)) |
| `stock.picking.type` | `base.group_user` read ([stock csv#L9](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L9)). **This answers R1 UNVERIFIED #8** | — |
| `stock.move` | `stock.group_stock_user` ([stock csv#L14](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L14)); `account.group_account_readonly` ([stock_account csv#L7](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock_account/security/ir.model.access.csv#L7)); purchase and sale users ([purchase_stock csv#L7](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/security/ir.model.access.csv#L7), [sale_stock csv#L3](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/security/ir.model.access.csv#L3)) | Company rule |
| `stock.move.line` | **`base.group_user` 1,1,1,1** ([stock csv#L33](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L33)) | Company rule |
| `stock.quant` | **`base.group_user` read** ([stock csv#L22](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L22)) | `company_id in company_ids + [False]` ([stock_security.xml#L122-L123](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/stock_security.xml#L122-L123)) |
| `stock.location`, `stock.warehouse`, `stock.route`, `stock.rule` | `base.group_user` read ([stock csv#L3](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L3), [#L6](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L6), [#L29-L30](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L29-L30)) | Company rules |
| `stock.warehouse.orderpoint` | `stock.group_stock_user` read ([stock csv#L19](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L19)); `purchase.group_purchase_user` read ([purchase_stock csv#L13](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/security/ir.model.access.csv#L13)); `sales_team.group_sale_salesman` read ([sale_stock csv#L11](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/sale_stock/security/ir.model.access.csv#L11)). **Write only for `stock.group_stock_manager`** ([stock csv#L20](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/stock/security/ir.model.access.csv#L20)) | Company rule |
| `product.supplierinfo` | `base.group_user` read ([product csv#L4](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/security/ir.model.access.csv#L4)) | — |
| `mail.message` | `base.group_user` (R2-O9) | Custom message access, **UNVERIFIED** |
| `ir.module.module`, `base.automation`, `mail.tracking.value` | `base.group_system` only (R2-O9) | — |

- **Group implications:** `account.group_account_readonly` implies `base.group_user` ([account_security.xml#L50-L53](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/security/account_security.xml#L50-L53)).
- **Candidate read-only bot** (derived from the ACL lines above, not tested): `account.group_account_readonly` + `stock.group_stock_user` reads every model in this table except `ir.module.module`, `base.automation` and `mail.tracking.value`. It avoids the salesman own-orders rule and the purchase-user vendor-only rule.
  - `stock.group_stock_user` carries write rights on pickings, moves and quants. A strictly read-only stock group was not found.
  - Calling `action_replenish` needs the rights of whatever it touches. That set is **UNVERIFIED**.
  - **WF-01's existing bot groups are not changed by this pack.**

---

## R2-O11. Odoo 20 JSON-2 doc: differences that affect these items — **PARTIAL** (doc summaries, not verbatim)

Both [DOC] and [DOC20] were read on 2026-11-20 (HTTP 200).

| Topic | 19.0 vs 20.0 |
|---|---|
| URL shape, headers, body, error object, status codes, Custom-plan-only availability | **No difference found** |
| Transactions (one call = one transaction) | Same meaning, wording differs |
| API keys | Both: maximum three months. 20.0 adds that expiry is checked against the user's role limits. In 19.0 the same rule appears in the programmatic `generate()` text (see `odoo-19.md` §0) |
| Aggregation (`read_group` / `formatted_read_group`), `fields_get`, webhooks, rate limits | **Not mentioned on either page.** Webhooks appear only as a sidebar link |
| Deprecations | 20.0: XML-RPC `db` service listed as removed in "Odoo 20 (fall 2026)". `common` / `object` services scheduled for removal in Odoo 22. **No effect on JSON-2** |

- **No difference found affects R2-O1…R2-O10.**
- Odoo 20 **source** (field names, decorators) was **not** checked. That is **UNVERIFIED** (see list).

---

## UNVERIFIED (Odoo, R2)

1. Live behaviour of everything above. No Custom-plan Odoo 19 instance is available (gate L1).
2. The technical field name and selection keys of 3-way matching's "Should Be Paid", whether it blocks payment registration, and which edition provides `account_3way_match`. The module is absent from `odoo/odoo` 19.0.
3. The verbatim wording of the doc pages [DOC-BILLS], [DOC-AUTO], [DOC-HOOKS], [DOC20]. Only summaries were read.
4. That JSON-2 `context` keys (`warehouse_id`, `location`, `to_date`, `tz`) produce the scoped quantities and tz-grouped results described, when tested live.
5. The record-level access rule for `mail.message` (custom `_check_access`), and what a non-admin bot can read on chatter.
6. The effective rows a bot sees when it holds several groups with different record rules (for example `purchase.group_purchase_user` together with `account.group_account_readonly` on `account.move`).
7. The rights needed to call `stock.warehouse.orderpoint.action_replenish` with a least-privilege bot (it creates POs as SUPERUSER inside, but the orderpoint write needs `stock.group_stock_manager`).
8. Which cron runs `stock.rule._run_scheduler_tasks`, and its default interval.
9. Whether a webhook (`ir.actions.server` `webhook`) fires for writes made through JSON-2 by the bot itself (loop risk).
10. Odoo 20 source differences for the fields and methods in this pack. Only the doc page was compared.
11. `sale.order.line` invoice-status and delivered-quantity compute rules (`_compute_qty_delivered`, `_compute_invoice_status` on lines). Fields are listed; formulas not traced.
12. Rate limits and idempotency (still silent in [DOC] and [DOC20]; carried over from `odoo-19.md`).
