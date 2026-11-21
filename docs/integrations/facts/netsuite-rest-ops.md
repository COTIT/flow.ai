# NetSuite REST facts pack (R2: WF-02..WF-07 records, SuiteQL, change detection, permissions)

Retrieved 2026-11-21. This pack holds facts only. Each fact cites an official Oracle page. Anything a source does not state is marked **UNVERIFIED**. When two official sources disagree, both quotes are given and the item is **PARTIAL**. SOAP-only statements are not used for REST facts.

**Sources**
- **Help Center:** pages under `https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/<page>.html`, written below as `NS/<page>`.
- **REST API Browser (RAB):** record service v1, version **2026.1**, at <https://system.netsuite.com/help/helpcenter/en_US/APIs/REST_API_Browser/record/v1/2026.1/index.html>.
  - The page is a JS loader. Its 28 static `chunks/data00.part`…`data27.part` files were downloaded and parsed for `#definition-<record>`, `#definition-<record>-<sublist>Element` and `#operation--<record>-*`.
  - RAB marks no field as required or read-only in the parsed HTML (same as the R1 pack). Field types are quoted as the RAB shows them; a reference is an object `{id, refName}`.
- **Help Center index:** `NS/toc.htm` (the Help Center table of contents) was used to find page IDs.

**What each verdict means**
- **VERIFIED:** read directly in the cited source.
- **PARTIAL:** some of it is verified and the rest is listed as UNVERIFIED.
- **UNVERIFIED:** no official statement was found.

No NetSuite account was used. Every behaviour below is the documented contract, not observed behaviour.

---

## 0. Shared: base URL, auth, M2M, errors, concurrency

Not repeated here. See `docs/integrations/facts/netsuite-rest.md`:
- §0 for base URL, paths, REST feature and base role permissions
- **JWT** for OAuth 2.0 client credentials (M2M)
- **N2** for external IDs and the `q=` filter grammar
- **N3** for the async idempotency key
- **N5** for concurrency
- **N6** for the error body
- **PATCH** for sublist semantics

Rules from the R1 pack that apply to every record below:
- "REST web services do not support the update of subrecords, and do not support the bulk creation of records" ([NS/section_1545141395](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545141395.html)).
- "REST web services do not support legacy tax features. To work with taxation through REST web services, you must have the SuiteTax feature enabled" ([NS/chapter_1558962745](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_1558962745.html)).

**Supported records list ([NS/chapter_1558962745](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_1558962745.html))**
- The list opens with "The following records are supported in REST web services."
- It includes, among others: Inventory Adjustment, Inventory Item, Inventory Number, Inventory Status, Item Fulfillment, Item Receipt, Item Supply Plan, Purchase Order, Vendor Bill, Vendor Payment, Transfer Order, Invoice, Sales Order.
- The list carries **no beta or GA label** for any record. The word "beta" does not appear on the page. GA vs beta is therefore **not stated** for any record in this pack.

**Transforms ([NS/section_157901123882](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157901123882.html))**
- "All record transformations available in SuiteScript are available in REST web services, too."
- URL form: `POST /record/v1/<type>/<id>/!transform/<targetType>`. Success is "HTTP 204 - No Content".
- The body may set fields of the new record.

---

## R2-N1. `itemReceipt` — **VERIFIED**

**Support ([NS/section_0817102411](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_0817102411.html))**
- Record ID: "The record ID for an item receipt REST record is itemreceipt." Subrecord: `inventorydetail`.
- "There are no prerequisites for using the record in REST."
- The same page also says: "This transaction is available when the Locations feature and the Multi-Location Inventory (MLI) feature are enabled."
  - Whether REST access to item receipts needs those two features is **UNVERIFIED**: the page states both sentences.
- Advanced Receiving decides the flow:
  - "If you do not use Advanced Receiving, the receiving and billing processes are combined. When you receive an item, you create a vendor bill for it simultaneously."
  - "With Advanced Receiving, you can use separate processes to receive items separately from billing items."
- **Sample** (create from a PO), with `line` choosing the PO line:

  ```
  POST {{REST_SERVICES}}/record/v1/purchaseorder/2173/!transform/itemreceipt
  {"item":{"items":[{"line":1,"inventorydetail":{…}}]}}
  ```

**Operations (RAB 2026.1)**

| Operation | Path |
|---|---|
| Create from PO | `POST /purchaseOrder/{id}/!transform/itemReceipt`. Returns 204 "Transformed record", 202 if async. `Accept: application/vnd.oracle.resource+json; type=create-form` returns 200 with a prefilled form and does not submit it |
| Insert | `POST /itemReceipt` |
| Get / Upsert / Update / Delete | `GET`/`PUT`/`PATCH`/`DELETE /itemReceipt/{id}` |
| List | `GET /itemReceipt` |
| Bulk delete | `DELETE /itemReceipt?ids=` ("Delete a list of records") |
| Attach / detach | contact |

No `!transform` *from* `itemReceipt` is listed in RAB.

**Fields (RAB `definition-itemReceipt`, `itemReceipt-itemElement`)**

| Level | Field | RAB type |
|---|---|---|
| Body | `createdFrom` | `nsResource` (the link to the source PO) |
| Body | `entity`, `subsidiary`, `location`, `currency` | references |
| Body | `exchangeRate` | number (double) |
| Body | `tranDate` | string (date) |
| Body | `tranId` | string, "Ref No." |
| Body | `orderId` | integer (int64), "Order Id" |
| Body | `orderType` | string |
| Body | `externalId` | string |
| Body | `createdDate`, `lastModifiedDate` | string (date-time) |
| Body | `item` | `itemReceipt-itemCollection` |
| Body | `landedCostMethod` | enum `WEIGHT, QUANTITY, VALUE` |
| Line | `itemReceive` | boolean, "Fulfill/Receive" |
| Line | `quantity` | number (float) |
| Line | `quantityRemaining` | number (float) |
| Line | `onHand` | number (float) |
| Line | `orderLine` | integer (int64), "Order Line" |
| Line | `orderDoc` | `nsResource` |
| Line | `line` | integer (int64), "Transaction Line" |
| Line | `item` | reference |
| Line | `location` | reference |
| Line | `rate` | number (double) |
| Line | `itemUnitPrice` | number (double) |
| Line | `units` | string |
| Line | `billVarianceStatus` | enum `NOVARIANCES, JOURNALNOTPOSTED, JOURNALPOSTED` |
| Line | `inventoryDetail` | subrecord |
| Line | `isDropShipment` | boolean |

**SuiteQL:** the receipt is a row of the `transaction` table (see R2-N8). Its type code and line columns are **UNVERIFIED**.

---

## R2-N2. `vendorBill` — **PARTIAL**

**Support ([NS/article_164484956387](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_164484956387.html))**
- "NetSuite exposes the vendor bill record to REST web services." Record ID `vendorBill`. Subrecord `billingAddress`.
- "The following elements are not accessible through REST web services: expensePlanMessage accountingBookDetail taxDetails".
  - RAB still defines `accountingBookDetail` and `taxDetails` on `vendorBill`, the same pattern as the R1 PO contradiction.
- Samples on the page: create `POST /vendorBill` with `{"entity":{"id":…},"item":{"items":[{"item":{"id":…},"rate":10}]}}`; `GET …?expandSubResources=true`; `PATCH`; `DELETE`.
- 2026.1: setting `grossAmt` on a taxable line recalculates the line `amount`.

**Operations (RAB 2026.1)**

| Operation | Path |
|---|---|
| Create from PO | `POST /purchaseOrder/{id}/!transform/vendorBill` |
| Insert / Get / Upsert / Update / Delete / List / Bulk delete | as R2-N1 |
| Transforms **from** a bill | `POST /vendorBill/{id}/!transform/vendorCredit`, **`/!transform/vendorPayment`**, `/!transform/vendorReturnAuthorization` |

**Link to the PO**
- The RAB `vendorBill` body has **no `createdFrom` field**. It has no `purchaseOrder` field either.
- The lines carry `orderDoc` (`nsResource`) and `orderLine` (integer, "Order Line"). Line `billReceipts` is an `itemReceiptCollection`.
- RAB gives no description for `orderDoc`, `orderLine` or `billReceipts`. That they point to the PO, the PO line and the matched receipts is **UNVERIFIED** (the names suggest it; no text states it).

**Body fields (RAB `definition-vendorBill`)**

| Field | RAB type / text |
|---|---|
| `approvalStatus` | object, `id` enum `11, 1, 2, 3` (meanings not given) |
| `nextApprover` | `employee` |
| `status` | object, `id` enum `A, B, C, D, E, F` (meanings not given) |
| `documentStatus` | string, "Status" |
| `paymentHold` | boolean, "Payment Hold" (see R2-N4) |
| `entity`, `subsidiary`, `currency`, `location`, `account`, `terms` | references |
| `exchangeRate` | number (double) |
| `tranDate`, `dueDate`, `discountDate` | string (date) |
| `tranId` | string, "Reference No." ("such as the vendor's invoice number") |
| `transactionNumber` | string |
| `total` | number (double), "Amount" |
| `userTotal`, `taxTotal`, `balance`, `discountAmount` | number (double) |
| `received` | boolean, "Received Bill" |
| `externalId` | string |
| `createdDate`, `lastModifiedDate` | string (date-time) |
| `item`, `expense`, `installment`, `landedCosts` | collections |

**Line fields (RAB `vendorBill-itemElement`)**
- `item`, `quantity` (float), `rate` (double), `amount` (double), `grossAmt`, `taxAmount`, `units` (string), `line` (int64), `uniqueKey` (int64).
- `orderDoc`, `orderLine`, `billReceipts`, `billVarianceStatus` (enum `NOVARIANCES, JOURNALNOTPOSTED, JOURNALPOSTED`), `isClosed`, `location`, `subsidiary`, `linked` (boolean).

**Fields and operations that act on payment** (as documented; the R2-N4 pages give the meaning)

| Item | Effect stated by Oracle |
|---|---|
| `vendorBill.paymentHold = true` | "a vendor payment for that bill cannot be processed" (RAB) |
| `vendorBill.approvalStatus` | Pending Approval: "Someone with permission must approve the bill before a payment can be processed for it". The REST enum id for Approved is **UNVERIFIED** |
| `POST /vendorBill/{id}/!transform/vendorPayment` | creates a vendor payment from the bill (RAB) |
| `POST /vendorPayment` with `apply.items[{doc:{id:<bill>},apply:true,amount}]` | creates a payment applied to the bill ([NS/article_7095737506](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_7095737506.html)) |
| `vendorPayment.approvalStatus` | object, `id` enum `11, 1, 2, 3`. `vendorPayment.status` enum `A, B, C, D, E, V, F, Z` (RAB). Meanings not given |

---

## R2-N3. PO line received / billed quantities — **PARTIAL**

**RAB `purchaseOrder-itemElement` (record API)**

| Field | RAB type |
|---|---|
| `quantity` | number (float) |
| `quantityReceived` | number (float), "Quantity Received" |
| `quantityBilled` | number (float), "Billed" |
| `quantityOnShipments` | number (float) |
| `quantityFulfilled` | number (float) |
| `quantityAvailable`, `quantityOnHand`, `quantityCommitted` | number (float) |
| `matchBillToReceipt` | boolean, "Match Bill To Receipt" |
| `billVarianceStatus` | enum `NOVARIANCES, JOURNALNOTPOSTED, JOURNALPOSTED` |
| `isClosed` | boolean |
| `expectedReceiptDate` | string (date) |
| `createdFrom` (line) | `blanketPurchaseOrder` |
| `linkedOrder` | `purchaseRequisitionCollection` |

**Gaps**
- Whether `quantityReceived` and `quantityBilled` are read-only, and whether they count pending-approval bills, is **UNVERIFIED** in RAB.
- [NS/section_N2373552](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2373552.html) states that a Pending Approval bill "Counts towards outstanding bill quantity on the purchase order: Yes", and a Rejected one does not.
- The SuiteQL `transactionLine` column names for received and billed quantity are **UNVERIFIED**. The Records Catalog is in-account only.

---

## R2-N4. Native matching, variance and payment controls — **PARTIAL**

### a. 3 Way Match Vendor Bill Approval Workflow (SuiteApp)

- **What it is** ([NS/section_4096219721](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_4096219721.html)): "checks the vendor bill for discrepancies before it is processed for payment. It validates the details of a vendor bill against the details of its corresponding purchase order and item receipt. Bills with identified discrepancies are automatically routed to the assigned supervisor for review and approval."
- **Shipped in:** "the NetSuite Approvals Workflow SuiteApp".
- **Limitation:** "Item receipts that have been partially received are not supported."
- **Turn on** ([NS/section_4096250993](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_4096250993.html)):
  - Enable Features: A/P (Accounting), Purchase Orders and Advanced Receiving (Transactions), Approval Routing (Employees), SuiteFlow (SuiteCloud).
  - Accounting Preferences > Approval Routing > **Vendor Bills**.
  - Install bundle "NetSuite Approvals Workflow", Bundle ID **537941**.
  - The workflow's default status is "Not Running". It must be set to **Released**.
  - The install adds "23 saved searches" for the exception criteria.
- **Approver permissions:** "at least the Edit level of access" for **Vendor Bill Approval** and **Bills** (Transactions subtab).
- **What it blocks** ([NS/section_4096454192](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_4096454192.html)):
  - A bill entering Bill Validation is set to Pending Approval.
  - With exceptions it goes to the Pending Approval state, where "The record is locked for editing, when waiting for the supervisor's approval or rejection".
  - Without exceptions "It is automatically approved".
  - If "Bill in Advance of Receipt" is enabled, "exceptions that validate against the item receipt are no longer executed".
- **Exception criteria** ([NS/section_4096471509](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_4096471509.html)):
  - VB-PO Terms, Location, Receipt of Item, VB Standalone.
  - VB-PO and VB-IR quantity tolerance and difference at Vendor, Item and Subsidiary level.
  - VB qty greater or less than PO qty.
  - VB-PO amount tolerance at Vendor, Item and Subsidiary level.
  - VB amount greater or less than PO amount.
  - "A tolerance limit determines what percentage of the evaluated value is used as the limit. A difference limit represents an absolute quantity number that is used as the limit."

**Item-level tolerance fields**
- Help page: [NS/section_1504284393](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1504284393.html). REST names from RAB `definition-inventoryItem`; all are number (float).

| RAB field | Label |
|---|---|
| `purchaseOrderQuantity` | Vendor Bill - Purchase Order Quantity Tolerance |
| `purchaseOrderAmount` | Vendor Bill - Purchase Order Amount Tolerance |
| `purchaseOrderQuantityDiff` | Vendor Bill - Purchase Order Quantity Difference |
| `receiptQuantity` | Vendor Bill - Item Receipt Quantity Tolerance |
| `receiptAmount` | Vendor Bill - Item Receipt Amount Tolerance |
| `receiptQuantityDiff` | Vendor Bill - Item Receipt Quantity Difference |

**Gaps**
- The vendor-level and subsidiary-level tolerance field names on REST are **UNVERIFIED**; not checked in RAB.
- Whether the workflow triggers on bills created or edited through REST is **UNVERIFIED**. This is the same gap as R1 N4, live gate L4.
- The SuiteScript execution-context page says user event scripts and workflows can be filtered to the REST context ([NS/section_157927488310](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157927488310.html)). It does not say what the SuiteApp's deployment uses.

### b. Standard vendor bill approval (no SuiteApp)

Sources: [NS/section_N2373552](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2373552.html), [NS/section_N2374176](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2374176.html), [NS/section_N2374450](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2374450.html).

- **Turn on:**
  - "When you use the Payables feature, your vendor bills show an Approval Status field."
  - Setup > Accounting > Accounting Preferences > **Default Vendor Bill Status**.
- **Effect of the preference value:**
  - Approved: "No further review or processing is required before a payment is processed."
  - Pending Approval: "Someone with permission must approve the bill before a payment can be processed for it."
- **Status impact table:**

  | Status | Accounting impact | Counts towards outstanding bill qty on the PO |
  |---|---|---|
  | Pending Approval | None | Yes |
  | Rejected | None | None |
  | Approved | Yes | Yes |

- **Approving:** set Approval Status to Approved; "The bill can be processed for payment."
- **Cancelling:** "A vendor bill that is canceled is not processed for payment."
- **Permissions:** "Vendor bill approval requires the Vendor Bill Approval, Bills, and Find Transaction permissions."
- **One bill per PO item:** "After a vendor bill is entered against a purchase order, an additional bill can't be entered against the same purchase order items. This is true regardless of the status of that vendor bill."
- **Custom routing:** the Vendor Bill preference for Approval Routing plus a SuiteFlow workflow.

### c. Vendor bill variances (Advanced Receiving)

Sources: [NS/section_N2371184](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2371184.html), [NS/section_N2372745](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2372745.html).

- **Prerequisite:** "If you use the Advanced Receiving feature, you can use the vendor bill variances process to generate journal postings to variance accounts."
- **Variance types:** Bill Quantity Variance, Bill Price Variance, Bill Exchange Rate Variance. They are posted at Transactions > Payables > Post Vendor Bill Variances.
- **Two methods:**
  - **Match Bill to Receipt**, a per-PO-line box (`matchBillToReceipt` in RAB) that can default from the item record. Variances are based on bill lines against linked receipts.
  - **PO-line based**, for closed PO lines without the box.
- **Lock after posting:** "After the variances on the journal entry are created and posted, associated purchase orders, receipts, and vendor bills transactions can't be changed. The variance journal entries must be voided or deleted to allow changes."
- **Pending bills on the page:** "Vendor bills in pending approval status appear on this page".
- **REST:** no REST operation for "Post Vendor Bill Variances" was found. Line `billVarianceStatus` exposes the result (RAB).

### d. Payment hold on vendor bills

- **RAB text for `vendorBill.paymentHold`:** "Check the Payment Hold box to apply a payment hold on a disputed bill: * The Make Payment button does not appear on the vendor bill and a vendor payment for that bill cannot be processed. * Vendor bills subject to a payment hold do not appear on the bill payments page. * The Electronic Payment SuiteApp does not process payments that are subject to a payment hold."
- **Gaps (UNVERIFIED):**
  - which feature or preference exposes the field
  - which permission may set or clear it
  - whether a REST `POST /vendorPayment` applying a held bill is rejected
- [NS/section_N1570238](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N1570238.html) ("Override Payment Holds") covers **sales-order** payment holds, not vendor bills. It is not applied here.

### e. Vendor payment approval

- [NS/section_1006010604](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1006010604.html) (Payment Automation SuiteApp): "Starting with Payment Automation version 2.0, batch approval routing is required". NetSuite approval routing for payments is optional.
- Approval routing is turned off at Enable Features > Employees > Approval Routing, "only … if there are no pending vendor payments for approval".
- How `vendorPayment.approvalStatus` behaves on REST create is **UNVERIFIED**.

---

## R2-N5. Inventory quantities by item and location — **PARTIAL**

**Definitions ([NS/section_N2262573](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2262573.html))**

| Quantity | Oracle definition |
|---|---|
| Quantity Available | "Quantity On Hand less Quantity Committed". "Quantity Available is never tracked as a negative quantity." |
| Quantity On Hand | "Quantity currently stocked, including Quantity Committed" |
| Quantity Committed | "Quantity promised to customers on approved sales orders that are not yet fulfilled" |
| Quantity On Order | "Quantity on approved purchase orders pending receipt from the vendor" |
| Quantity To Order | "Preferred Stock Level less Quantity Available" |
| Quantity Back Ordered | "Quantity committed to sales for which there is no stock to fill the order" |
| Reorder Point | "Quantity level at which you need to place an order to replenish stock" |

- "Additional quantities, such as quantity in transit and quantity allocated, may appear depending on features."
- **Commitment depends on a preference:** "Committing items to orders can be done automatically or manually based on your setting for the Perform Item Commitment after Transaction Entry preference" ([NS/section_3765928460](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_3765928460.html)).
- **Multi-location** ([NS/section_N2305626](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2305626.html)): reorder point and preferred stock level are set per location on the item's Locations subtab. The Items list "Stock" view shows totals across all locations.

**Record API (RAB)**

| Where | Fields (all number (float) unless noted) |
|---|---|
| `inventoryItem` body (all locations) | `quantityOnHand` ("the quantity of this item you have on hand"), `quantityAvailable`, `quantityCommitted` ("already committed to be sold and are currently showing on orders"), `quantityOnOrder` ("currently have on order with the vendor"), `quantityBackOrdered`, `quantityOnHandBase`, `quantityAvailableBase` |
| `inventoryItem-locationsElement` (per location) | `location`, `locationId` (int64), `quantityOnHand`, `quantityAvailable`, `quantityCommitted`, `quantityOnOrder`, `quantityBackOrdered`, `quantityInTransit`, `qtyInTransitExternal`, `reorderPoint`, `preferredStockLevel`, `safetyStockLevel`, `leadTime` (int64, "Purchase Lead Time"), `averageCostMli`, `lastPurchasePriceMli` |
| `inventoryAdjustment-inventoryElement` | `quantityOnHand` ("Qty On Hand") |

Item record limits on REST ([NS/article_161522936052](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_161522936052.html)): it "does not show Quantity Pricing Discount/Levels … or the Item Translations subtab". The sample reads with `GET /inventoryItem/<ID>?expandSubResources=true`.

**SuiteQL tables**
- `query.Type` lists `ITEM` → `item`, `ITEM_LOCATION_CONFIGURATION` → `itemlocationconfiguration`, `LOCATION` → `location`, `INVENTORY_STATUS` → `inventorystatus` and `PICK_TASK_INVENTORY_BALANCE` → `picktaskinventorybalance` ([NS/section_1510878994](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1510878994.html)).
- That page says query types follow the SuiteAnalytics Workbook record types, and "A query type is not the same as a record type."
- **Not listed in `query.Type`:** `inventoryBalance`, `aggregateItemLocation`, `inventoryItemLocations`. Their existence and columns as SuiteQL tables are **UNVERIFIED**.
  - The saved-search type "Inventory Balance" exists in the UI ([NS/section_1518564230](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1518564230.html)). That is not a SuiteQL statement.
- The columns of `itemlocationconfiguration` are **UNVERIFIED**.

---

## R2-N6. `inventoryAdjustment` — **PARTIAL**

**Support ([NS/section_0817101055](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_0817101055.html))**
- Record ID `inventoryadjustment`. Subrecord `inventorydetail`.
- "This transaction is available when the Inventory feature is enabled." "There are no prerequisites for using the record in REST."

**REST can create it — VERIFIED**
- Documented sample:

  ```
  POST {{REST_SERVICES}}/record/v1/inventoryadjustment
  {"account":{"id":"1"},"adjLocation":{"id":"1"},"customer":{"id":"19"},
   "inventory":{"items":[{"adjustQtyBy":5.0,"item":{"id":"20"},"line":1,"location":{"id":"1"}}, …]},
   "postingPeriod":{"id":"194"},"subsidiary":{"id":"1"}}
  ```

- RAB operations: `POST /inventoryAdjustment`, `GET`/`PUT`/`PATCH`/`DELETE /inventoryAdjustment/{id}`, list, and bulk delete. No transforms.

**Fields (RAB)**
- **Body:** `account`, `adjLocation`, `location`, `subsidiary`, `postingPeriod`, `customer`, `department`, `class`, `tranDate`, `tranId`, `memo`, `externalId`, `estimatedTotalValue`, `inventory` (collection), `createdDate`, `lastModifiedDate`.
- **Line `inventoryAdjustment-inventoryElement`:**
  - `item`, `location`, `line`, `units` (string), `description`, `memo`, `inventoryDetail`, `binNumbers`, `serialNumbers`
  - `adjustQtyBy` (float, "Adjust Qty By"), `newQuantity` (float, "New Qty"), `quantityOnHand` (float)
  - `unitCost` (double), `currentValue` (double)
  - `costingMethod` (enum `SERIAL, LOT, AVG, LIFO, GROUPAVG, FIFO, STANDARD`)

**Permission for create — UNVERIFIED**
- A permission named **"Adjust Inventory"** appears in the Standard Roles Permissions Table ([NS/section_N295396](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N295396.html)).
- No REST page says which permission and level a REST create needs. Oracle points to `NetSuitePermissionsUsage.xls` from [NS/section_N326485](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N326485.html) as the usage map. That file was not downloaded. Live gate.

---

## R2-N7. `salesOrder`, `itemFulfillment`, `invoice` — **VERIFIED**

**Sales order ([NS/section_159665260887](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_159665260887.html))**
- Record ID `salesorder`.
- **Transforms (RAB):** `POST /salesOrder/{id}/!transform/` + `itemFulfillment` | `invoice` | `cashSale` | `fulfillmentRequest` | `returnAuthorization`.
- **Fulfillment from a sales order:** "When you transform a sales order into an item fulfillment, you must include the inventoryLocation field in the request body."
  - The sentence links to "Transforming Sales Orders with the Intercompany Cross-Subsidiary Feature". Whether it applies outside that feature is **UNVERIFIED**.
- **Body (RAB):** `createdFrom` (`nsResource`), `entity`, `subsidiary`, `currency`, `exchangeRate` (double), `tranDate`, `shipDate`, `tranId` ("Order #"), `externalId`, `subtotal`, `total`, `createdDate`, `lastModifiedDate`.
  - `status` enum `A, B, C, D, E, F, G, H`; `orderStatus` enum `A, B`. Meanings not given.
- **Line (RAB `salesOrder-itemElement`):** `quantity`, `quantityCommitted`, `quantityFulfilled`, `quantityBilled` ("Invoiced"), `quantityBackOrdered`, `quantityAvailable`, `quantityOnHand`, `quantityPicked`, `quantityPacked`, `isClosed`.

**Item fulfillment ([NS/section_161425629582](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_161425629582.html))**
- Record ID "ItemFulfillment". Subrecord InventoryDetail. "There are no prerequisites".
- "If Advanced Shipping is not enabled, the fulfillment and invoicing processes are combined. When an item fulfillment is created, a related invoice is created at the same time."
- **Sample:** `POST …/salesorder/{id}/!transform/itemFulfillment` with lines `{"orderLine":1,"location":6,"itemreceive":true}`.
- **Body (RAB):** `createdFrom` (`nsResource`), `createdFromShipGroup`, `entity`, `subsidiary`, `tranDate`, `tranId`, `exchangeRate` (float), `externalId`, `lastModifiedDate`.
  - `status` and `shipStatus` both have enum `A, B, C`. Meanings not given.
- **Line:** `itemReceive` ("Fulfill/Receive"), `quantity`, `quantityRemaining`, `onHand`, `orderLine`, `orderDoc`, `location`.
- **RAB operations:** CRUD, list and bulk delete. No transforms *from* fulfillment.

**Invoice ([NS/section_161488248489](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_161488248489.html))**
- The page says "The invoice record exposes an invoice to REST web services".
- **Body (RAB):** `createdFrom` (`nsResource`), `entity`, `currency`, `exchangeRate` (double), `tranDate`, `dueDate`, `tranId` ("Invoice #"), `total`, `subtotal`, `amountPaid`, `amountRemaining`, `externalId`, `lastModifiedDate`.
  - `status` enum `A, B, D, E, V`; `approvalStatus` enum `11, 1, 2, 3`. Meanings not given.
- **Line:** `orderDoc`, `orderLine`, `isClosed`.
- **Transforms (RAB):** `creditMemo`, `customerPayment`, `returnAuthorization`.
- **Transform sample:** `POST /salesOrder/3/!transform/invoice` returns 204 ([NS/section_157901123882](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157901123882.html)).

---

## R2-N8. SuiteQL `transaction` / `transactionLine`, dates, currency, paging — **PARTIAL**

**Tables**
- `query.Type` lists `TRANSACTION` → `transaction` ([NS/section_1510878994](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1510878994.html)).
- `transactionLine` appears in a SuiteQL syntax example, `SELECT * FROM transaction, transactionLine` ([NS/section_156257790831](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156257790831.html)).
- `CURRENCY` → `currency`, `CURRENCY_RATE` → `currencyrate`, `TRANSACTION_STATUS` → `transactionstatus`, `TRANSACTION_HISTORY` → `transactionhistory` (same `query.Type` page).
- **Column names** (`type`, `trandate`, `currency`, `exchangerate`, `createdfrom`, line quantities) and **`transaction.type` codes** are **UNVERIFIED**.
  - They live in the in-account Records Catalog, which "is available for all users with the Records Catalog permission" ([NS/article_159367781370](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_159367781370.html)).
  - "The Records Catalog is supported for the SuiteScript Analytic API and the REST Query API only" ([NS/subsect_160397886310](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_160397886310.html)).
  - The SuiteQL limitations page shows `TranType = 'CuTrSale'` for custom types only ([NS/section_156257796125](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156257796125.html)).
- **Casing:** "The casing of record type names and field names in SuiteQL query results may not always be consistent and can change … make sure that you don't depend on name casing" (same page).

**Dates and time zones**
- **REST record API** ([NS/section_157305090645](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157305090645.html)):
  - "Datetime values are returned in UTC format." ISO 8601, for example `2017-07-21T17:32:28Z`. A time zone may be sent, for example `+01:00`.
  - "Date field values are not subject to any time zone conversion."
  - "During record collection filtering, REST web services uses the underlying N/query SuiteScript module, which uses a different date format". The filter examples use `"1/1/2019"`, as in `dateCreated ON_OR_AFTER "1/1/2019"` ([NS/section_1545222128](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545222128.html)).
  - Date operators: `AFTER, BEFORE, ON, ON_OR_AFTER, ON_OR_BEFORE` and their `_NOT` forms.
- **SuiteQL** ([NS/article_0824094533](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0824094533.html)):
  - "You can't use date literals. You must encapsulate dates using the to_date() function."
  - Example: `where lastmodifieddate >= TO_DATE('2024-01-01 00:00:00', 'yyyy-mm-dd hh24:mi:ss')`.
  - "Do not use plain string values for date comparisons."
  - "you can't use more than 1000 arguments in a single IN clause"; no `WITH` clauses; no right outer joins in Oracle syntax.
- **SuiteQL functions** ([NS/section_158513731864](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_158513731864.html)): `CURRENT_DATE` "returns the current date in the session time zone". `FROM_TZ`, `SYS_EXTRACT_UTC`, `NEW_TIME` and `TO_TIMESTAMP_TZ` are listed.
- **What "session time zone" is for REST SuiteQL**, and in what zone `trandate` and `lastmodifieddate` come back in SuiteQL results, is **UNVERIFIED**.

**Currency**
- Record API: `currency` (reference) and `exchangeRate` (double) on every transaction above (RAB). `inventoryAdjustment` lines have `exchangeRate` as float.
- The SuiteQL columns for the base-currency vs transaction-currency amount are **UNVERIFIED**.

**Paging and row limits**

| Limit | Value | Source |
|---|---|---|
| SuiteQL total rows | "Using SuiteQL queries, you can return a maximum of **100,000** results." | [NS/section_157909186990](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157909186990.html) |
| `Prefer: transient` | "Note that Prefer: transient is a required header parameter." | same |
| Page size, page count | "default setting of **1000** results per page on a maximum of **1000** pages" | [NS/section_156414087576](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_156414087576.html) |
| Max per page | "You can specify your criteria to return fewer than 1000 results per page. In this case, the overall number of returned results is lower, too, because a maximum of 1000 pages of results can be returned." | same |
| Total without Connect | "Without SuiteAnalytics Connect, you can display maximum **100 000** results." | same |
| Offset rule | "You can only set offset and limit values in a way that the offset is divisible by the limit." | same |
| Response fields | `count`, `offset`, `hasMore`, `totalResults`, `links` (`next`, `last`, `prev`, `first`, `self`) | same; SuiteQL sample shows `count`, `offset`, `totalResults`, `items` |
| Async | Existing pack: "Asynchronous request execution isn't supported in the REST query service." | [NS/subsect_1544786455](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_1544786455.html) |

- **Contradiction 1, total rows.** The paging page says 1000 × 1000 pages and "one million results if you use SuiteAnalytics Connect", but 100,000 without Connect. The page's own arithmetic does not give 100,000. Take **100,000** as the REST ceiling, since both pages state it.
- **Contradiction 2, async.** [NS/chapter_161251386312](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_161251386312.html) says "You can execute any REST request asynchronously by using the Prefer: respond-async request header". The limitations page excludes the query service.
- **Batching advice** (performance page): split large reads by `id` ranges. `id` and `lastmodifieddate` are named as "indexed fields".

---

## R2-N9. Item reorder fields and native replenishment — **VERIFIED**

**Item fields (RAB `definition-inventoryItem`)**

| Field | Type / Oracle text |
|---|---|
| `reorderPoint` | float. "The Reorder Point is the quantity level at which you need to reorder or build more of this item." |
| `autoReorderPoint` | boolean. Auto-calculate "based on demand for the item over time" |
| `preferredStockLevel` | float. "used to determine your replenishment needs on the Order Items page" |
| `autoPreferredStockLevel` | boolean |
| `safetyStockLevel`, `safetyStockLevelDays` | float / int64. "used to auto-calculate the reorder point" |
| `leadTime` | int64, "Purchase Lead Time": "the average number of days between ordering this item from the vendor and receiving it" |
| `autoLeadTime` | boolean |
| `reorderMultiple` | int64. "the Order Items page suggests ordering a quantity … always multiple of the number" |
| `quantityOnOrder` | float |
| `vendor` | `vendor` reference (body) |
| `cost` | double, "Purchase Price" |
| `itemVendor` | `inventoryItem-itemVendorCollection` |
| `supplyReplenishmentMethod` | enum `TIME_PHASED, MPS, REORDER_POINT, MRP` |
| `demandSource` | enum `FORECASTANDORDERS, EXISTINGORDERSANDDEPENDENTDEMAND, FORECASTCONSUMPTION, ITEMDEMANDPLAN` |
| `supplyLotSizingMethod`, `supplyType` | objects |
| `seasonalDemand` | boolean |

- **Per location** (`inventoryItem-locationsElement`): `reorderPoint`, `preferredStockLevel`, `safetyStockLevel`, `leadTime`, `quantityOnOrder`, `minimumOrderQuantity`, `fixedLotSize`, `supplyType` (`TRANSFER, PURCHASE`), `supplyLotSizingMethod`.
- **Vendor sublist** (`inventoryItem-itemVendorElement`): `vendor`, `preferredVendor` (boolean, "Preferred"), `purchasePrice` (double), `vendorCode`, `vendorCurrencyId`, `subsidiary`, `schedule`, `itemVendorPrice`.

**Native features**
- **Advanced Inventory Management** ([NS/chapter_N2285050](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_N2285050.html)): "helps you automate inventory replenishment based on demand. NetSuite uses transaction history and item record settings to calculate reorder points, preferred stock levels, and lead times." It covers Demand Planning, Distribution Resource Planning, Supply Planning, Supply Allocation and Available to Promise.
- **Order Items page** ([NS/section_N2403352](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2403352.html)):
  - Transactions > Inventory > Order Items.
  - "NetSuite suggests the quantity to order based on your preferred stock level and quantity of open backorders, or quantities scheduled on blanket purchase orders."
  - It lists items "at or below their reorder point", with filters by vendor ("Preferred Vendor on their item record") and location.
  - "NetSuite generates one purchase order per vendor".
- **Reorder reminder:** "When an item reaches its Reorder Point, a reminder is generated to purchase the item" ([NS/section_N2305626](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2305626.html)).
- **Demand Planning** ([NS/section_N2286970](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2286970.html)):
  - "To use Demand Planning you must enable Advanced Inventory Management."
  - "Purchase orders generated from supply plans use the preferred vendor from the item record."
  - "You can process up to 10,000 items at a time for Demand Planning functions."
  - **Item Supply Plan** is on the REST supported records list.
- **Time Phased** ([NS/article_162497068568](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_162497068568.html)): "The NetSuite Supply Planning (MRP) solution replaces Time-Phased Planning". Selecting Time Phased makes Reorder Point, Preferred Stock Level and related AIM fields unavailable.
- **REST gap:** no REST operation that runs the Order Items page or calculates supply plans was found. **UNVERIFIED**.

---

## R2-N10. Change detection and capability discovery — **PARTIAL**

| Option | Fact | Source |
|---|---|---|
| `lastModifiedDate` on records | Every record above has `lastModifiedDate` (string, date-time) in RAB. Usable in `q=` only if the field carries `x-ns-filterable` in metadata | RAB; [NS/section_1545222128](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545222128.html) |
| SuiteQL incremental load | Oracle's own example: `select id, lastmodifieddate, tranid from transaction where lastmodifieddate >= to_date('2024-01-01 00:00:00', 'yyyy-mm-dd hh24:mi:ss')`. "Using filters is a good option when doing incremental loads." | [NS/article_0824094533](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0824094533.html) |
| System notes in SuiteQL | `query.Type` has `SYSTEM_NOTE` → `systemnote` and `SYSTEM_NOTE2` → `systemnote2` | [NS/section_1510878994](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1510878994.html) |
| System notes permission | "you must have the Administrator role, the Core Administrator Permissions, or the System Notes for Analytics and REST permission. Without [them] … you can only see system notes for changes you made yourself." Not visible: actions not linked to a field (unless the "Include System Notes for Non-Field Actions in Analytics Data Source" preference is set), removed or renamed fields, hidden features | [NS/subsect_96115732745](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_96115732745.html) |
| System note content | "when and who made it, and what was changed"; v2 adds the role and the action (create, change, delete, set, update, unset) | [NS/article_160225379741](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_160225379741.html) |
| Metadata catalog | `GET /services/rest/record/v1/metadata-catalog` lists all exposed records, each with JSON, `application/swagger+json` (OpenAPI 3.0) and `application/schema+json` links. `?select=customer,salesorder` limits it. OpenAPI needs `Accept: application/swagger+json`. It describes record structure, supported operations, HTTP methods and query parameters. "Links to NetSuite records that are out of the scope of the select parameter are declared as generic JSON objects" | [NS/section_1540810174](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1540810174.html); [NS/section_1545126526](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_1545126526.html) |
| Async job status | `202` + `Location: /services/rest/async/v1/job/<id>`. `GET /async/v1/job/{id}` returns `{completed, id, progress:"pending", task{links}}`. `GET /async/v1/job/{id}/task/` returns task IDs. `GET /async/v1/job/{id}/task/{taskId}` returns `{completed, startTime, endTime, progress:"succeeded"}`. `GET …/task/{taskId}/result` returns the result body | [NS/subsect_161278579295](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_161278579295.html); [NS/subsect_161252896355](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_161252896355.html); [NS/subsect_161252889998](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_161252889998.html) |
| Push events | The REST API Guide's table of contents (overview, request processing, operations, metadata, records, sublists, subrecords, filtering and query, error handling) has **no** event, webhook or subscription topic. In-NetSuite reaction is documented via SuiteScript: "If a script has its execution context set to REST web services, the script only executes if it is triggered through a REST web services request" (user event and client scripts) | `NS/toc.htm` (REST guide section under [NS/chapter_1540391670](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/chapter_1540391670.html)); [NS/section_157927488310](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_157927488310.html) |

**Gaps**
- That REST web services have **no** push or outbound event mechanism at all is **UNVERIFIED**. No page says so. Only the absence of a topic was observed.
- The `systemnote` column names (record type, record id, field, old and new value, date) are **UNVERIFIED** (Records Catalog).
- Whether `systemnote.date` and `transaction.lastmodifieddate` share a time zone in SuiteQL output is **UNVERIFIED** (see R2-N8).

---

## R2-N11. Role permissions per record — **PARTIAL**

**Documented**
- **Base REST permissions:** see the R1 pack §0: REST Web Services (Full), Log in using OAuth 2.0 Access Tokens, SuiteAnalytics Workbook (Edit).
  - The R1 pack's §0 also says the REST page tells you to "consider which permissions you need … to access forms, sublists, fields" without naming them.
- **Records Catalog** permission, needed to see SuiteQL tables and columns in-account: "Permission > Setup > Add (Records Catalog)" ([NS/article_159367781370](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_159367781370.html)).
- **System Notes for Analytics and REST:** needed to read other users' system notes ([NS/subsect_96115732745](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/subsect_96115732745.html)).
- **Vendor bill approval (UI):** "Vendor Bill Approval, Bills, and Find Transaction" ([NS/section_N2373552](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N2373552.html)).
- **3-way-match approvers:** Vendor Bill Approval (Edit) and Bills (Edit) ([NS/section_4096250993](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_4096250993.html)).
- **Commit Orders page:** "the Commit Orders permission set to Full" ([NS/section_3765928460](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_3765928460.html)).
- **Permission names present** in the Standard Roles Permissions Table ([NS/section_N295396](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N295396.html)):
  - Purchase Order, Receive Order, Item Receipt, Bill Purchase Orders, Bills, Pay Bills, Vendor Bill Approval
  - Sales Order, Fulfill Orders, Item Fulfillment, Invoice
  - Adjust Inventory, Items, Find Transaction, Transfer Order
- **Levels shown:** View, Create, Edit, Full.

**UNVERIFIED (live gate L5)**
- Which permission and level each REST operation needs. That covers GET, POST, `!transform` and SuiteQL on `transaction` for `itemReceipt`, `vendorBill`, `vendorPayment`, `salesOrder`, `itemFulfillment`, `invoice`, `inventoryAdjustment` and `inventoryItem`.
- Oracle's usage map is the `NetSuitePermissionsUsage.xls` linked from [NS/section_N326485](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_N326485.html). It was not downloaded or read.
- The Connect "Record Names, Permissions, and Features Required" table ([NS/article_0411012830](https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/article_0411012830.html)) lists only price and sales-summary records. One example: `salesInvoiced` needs "Transactions: Invoice - view". It covers SuiteAnalytics Connect, not REST.

---

## UNVERIFIED (NetSuite, R2)

1. **All records:** GA vs beta. The supported records list has no label.
2. **R2-N1:** whether REST item receipts need Locations and MLI ("available when … enabled") or nothing ("no prerequisites"). The same page states both.
3. **R2-N2:** that `vendorBill` line `orderDoc`, `orderLine` and `billReceipts` point to the PO, PO line and receipts. RAB has no description, and the body has no `createdFrom`.
4. **R2-N2:** whether `accountingBookDetail` and `taxDetails` are usable. The guide says not accessible; RAB defines them.
5. **R2-N2:** meanings of `vendorBill.approvalStatus` (`11, 1, 2, 3`), `vendorBill.status` (`A`–`F`), `vendorPayment.status` and `vendorPayment.approvalStatus`.
6. **R2-N3:** whether PO line `quantityReceived` and `quantityBilled` are read-only, and their SuiteQL column names.
7. **R2-N4:** whether the 3 Way Match or Vendor Bill Approval workflow runs on bills created or edited through REST.
8. **R2-N4:** REST field names of the vendor-level and subsidiary-level tolerance fields.
9. **R2-N4:** which feature exposes `vendorBill.paymentHold`, which permission may change it, and whether a REST vendor payment against a held bill is rejected, with which error.
10. **R2-N4:** no REST operation found for Post Vendor Bill Variances.
11. **R2-N5:** SuiteQL tables `inventoryBalance`, `aggregateItemLocation` and `inventoryItemLocations` (not in `query.Type`), and the columns of `itemlocationconfiguration`.
12. **R2-N6:** the permission and level a REST `inventoryAdjustment` create needs ("Adjust Inventory" is a named permission, but the mapping is not stated).
13. **R2-N7:** whether `inventoryLocation` is required on every SO → fulfillment transform or only with intercompany cross-subsidiary. Also the meanings of the SO, fulfillment and invoice status letters.
14. **R2-N8:** `transaction` and `transactionLine` column names and `transaction.type` codes (Records Catalog only).
15. **R2-N8:** the SuiteQL session time zone for REST queries, and the zone of date and datetime columns in SuiteQL results.
16. **R2-N8:** the SuiteQL columns for base vs transaction-currency amounts.
17. **R2-N8:** total-row ceiling arithmetic. The paging page gives 1000 × 1000 and "maximum 100 000" in the same text.
18. **R2-N8:** async for SuiteQL. One page says "any REST request" can run async; another says the query service can't.
19. **R2-N9:** any REST operation to run Order Items or calculate supply plans.
20. **R2-N10:** whether REST web services offer any push or outbound events (no page found, absence only). Also `systemnote` column names.
21. **R2-N11:** the per-record, per-operation permission and level map for REST and SuiteQL (`NetSuitePermissionsUsage.xls` not read).
22. **Version:** RAB 2026.1 was read. The 2026.2 URL redirects to "page not found".
