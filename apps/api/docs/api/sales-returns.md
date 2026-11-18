# Sales returns

Flow ERP M4 (masterplan §12 CUSTOMER CREDIT / RESTOCK / CREDIT CORRECTIONS / CASH ROUNDING, §15 CUSTOMER RETURNS): credit lines of a recognised sale, put goods back on stock as a separate step, correct either side on its own. Base `/v1/workspace/{workspace}`, tag `sales-returns`, migration `sales_returns`. Active books only: every write in a setup book is 422 `unsupported_active_erp_operation`.

## Envelopes and replay

Same as shipments (`docs/api/shipments.md`): C `{client_ref, payload}` → 201, A `{client_ref, expected_version, payload}` → 200, required `Idempotency-Key`, permanent replay by `client_ref`, 409 `stale_document` when the version or a review fingerprint moved.

## Permissions

| Operation | Route | Permission |
|---|---|---|
| `sales_returns_list` | `GET /sales-returns?page=&page_size=` | `salesreturn.list` |
| `sales_returns_create` | `POST /sales-returns` | `salesreturn.create` |
| `sales_returns_get` | `GET /sales-returns/{return}` | `salesreturn.get` |
| `sales_returns_update` | `PATCH /sales-returns/{return}` | `salesreturn.update` |
| `sales_returns_cancel` | `POST /sales-returns/{return}/cancel` | `salesreturn.cancel` |
| `sales_returns_post_review` | `POST /sales-returns/{return}/post/review` | `salesreturn.post` |
| `sales_returns_post` | `POST /sales-returns/{return}/post` | `salesreturn.post` |
| `sales_returns_restock_review` | `POST /sales-returns/{return}/restock/review` | `salesreturn.post` |
| `sales_returns_restock` | `POST /sales-returns/{return}/restock` | `salesreturn.post` |
| `sales_returns_reverse_review` | `POST /sales-returns/{return}/reverse/review` | `salesreturn.reverse` |
| `sales_returns_reverse` | `POST /sales-returns/{return}/reverse` | `salesreturn.reverse` |
| `sales_return_restocks_reverse_review` | `POST /sales-returns/{return}/restocks/{restock}/reverse/review` | `salesreturn.reverse` |
| `sales_return_restocks_reverse` | `POST /sales-returns/{return}/restocks/{restock}/reverse` | `salesreturn.reverse` |
| `legacy_return_costs_create` | `POST /legacy-return-costs` | `accounting.post` |

A return, sale, restock or order of another workspace is 404.

## Credit

Create `{recognition_id, accounting_date, reason, lines: [{recognition_line_id, quantity}]}`; PATCH replaces date, reason and lines of a draft. `kind` is `invoice_credit` when the sale is billed on an issued invoice (or is a standalone invoice), else `receipt_credit` (anonymous Till receipts included; no customer, no invoice).

- Capacity, shared by every route (this one, order returns, credit notes): per recognition line, Σ draft + posted credits ≤ recognised quantity, under the recognition row lock. A draft reserves capacity only; cancel or reversal frees it.
- Post (review first; the accounting date must be the book's day): per line the cumulative proportional share of the recognised net and tax (`money_round_div`); the credit reaching the recognised quantity takes the exact rest. A Till receipt's cash rounding R (due − gross, from its `received` journal) is shared the same way by gross, so a full return gives back exactly the cash taken. Credit = net + tax + rounding.
- Journal `sales_return/<id>/posted`: DR `sales_returns` net, DR `tax_payable` tax, ± `rounding`, CR the sale's receivable role (billed `receivables` or `unbilled_receivables`) for the part applied at once to what the customer still owes, CR `receivables` for the rest.
- One open item, classification `customer_credit`, role `receivables`, source `sales_return/<id>`, `invoice_id` = the aliased credit note (else null), applied at once to the sale's open receivable. Its residual minus pending refund holds (`credit_refund_reservations`) is the refundable money; Till refunds allocate against it (`docs/api/till.md` "Till returns and refunds").

## Restock

Separate effect, posted credits only: `{lines: [{return_line_id, quantity, destination_warehouse_id, legacy_cost_evidence_id?}]}`. Goods go back into the lots the order took, at the original booked issue cost (`Valued::SaleReturn`, cost allocation of the sale, exact rest), DR `inventory` / CR `cogs`; with a reviewed legacy cost DR `inventory` / CR `inventory_adjustment`. Physical cap per line: min(line − its restocks, posted credits of the sale line − every restock of that sale line). A reversed credit keeps its restocks, so its replacement cannot restock the same units again.

Known limits: the destination must be the warehouse the sale left (other → 422; move it with a transfer). No bin. A sale with no booked cost needs `legacy_cost_evidence_id` (else 422 `legacy_return_cost_required`).

## Corrections

- Financial reverse: posted only, the book's day; refused 422 `dependent_effects_exist` while any allocation other than the post's own application uses the credit (refund, other application) or a pending refund hold sits on it. Reverses that application, posts the exact inverse journal (`sales_return/<id>/reversed`) and closes the credit item against a `sales_return_reversal` receivable item. State `reversed`; restocks stay.
- Restock reverse: exact inverse (`customer_return_reversal`, DR `cogs` or `inventory_adjustment` / CR `inventory`) only when no valued movement came after the restock in its pools and the goods are available and unreserved in their lots (else 422 `dependent_effects_exist` / `insufficient_stock`). The credit stays.
- The generic journal reverse stays manual-only.

## Legacy return costs

`{original_sale_id, line_id, quantity, restoration_amount, target_product_id?, evidence_ref, reason}`: a reviewed value for returns of an order line the book has no booked cost for. Quantity ≤ what the line delivered; amount in book currency (more decimals → 422); `target_product_id` must be the line's product (SKU mapping not supported). Restocks consume it cumulatively, the last unit takes the exact rest.

## Existing routes in active books

Setup books unchanged.

- `orders_returns`: permanent Idempotency-Key replay. Each returned line credits the order's recognitions oldest first as posted canonical returns (and restocks them into the order's warehouse when `restock`); the order's `returned` and status projections move as before. An order with no recognition (sold before activation) comes back physically only, valued at its line's reviewed legacy cost.
- `invoices/{id}/credit`: the draft credit note aliases one canonical `invoice_credit` draft (one sale per note). Items map to recognition lines by the sale's own invoice items (standalone) or billing position (shipment invoices); a mismatch is 422 and nothing is written. A `return_amount` must equal the proportional gross of its quantity (no repricing). The source invoice's status does not change at draft.
- Credit note issue posts the aliased credit once (no stock moves; restock is separate). A credit note without an alias stays 422. Deleting the draft note cancels the alias.
- `mark_as_credited` stays 422.

## Not yet

Office refunds, pending provider-refund holds, restock into another warehouse or bin, SKU mapping, goodwill/price-only credits, foreign-currency sales.
