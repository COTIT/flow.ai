# Odoo 19 facts pack (R1: O1–O6)

Retrieved 2026-11-17. This pack holds facts only. Each fact cites an official source. Anything a source does not state is marked **UNVERIFIED**.

**Sources**
- **Source code:** `odoo/odoo` branch `19.0`, pinned at commit [`61d8ae93ae5d`](https://github.com/odoo/odoo/tree/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f). Every line link below points at that commit, so line numbers stay stable.
  - Short form used below: `GH/<path>#Lx` = `https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/<path>#Lx`.
- **Docs:** "External JSON-2 API", Odoo 19.0, <https://www.odoo.com/documentation/19.0/developer/reference/external_api.html> (called **[DOC]** below).
- **Version:** `odoo/release.py` at this commit says `version_info = (19, 0, 0, FINAL, 0, '')` ([GH/odoo/release.py#L15](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/release.py#L15)).

**What each verdict means**
- **VERIFIED:** read directly in the cited source.
- **PARTIAL:** some of it is verified and the rest is listed as UNVERIFIED.
- **UNVERIFIED:** no official statement was found.

Nothing in this pack was run against a live Odoo instance. That is live gate L1.

---

## 0. JSON-2 transport (shared by every operation below)

| Aspect | Fact | Source |
|---|---|---|
| URL | `POST /json/2/<model>/<method>`. Any other path or verb under `/json/2` returns 404 with "Did you mean POST /json/2/<model>/<method>?" | [GH/addons/rpc/controllers/json2.py#L38-L56](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L38-L56) |
| Module | The controller lives in the `rpc` addon. Its manifest has `'auto_install': True` and depends only on `base` | [GH/addons/rpc/__manifest__.py#L10-L12](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/__manifest__.py#L10-L12) |
| Availability | [DOC]: "only available on Custom Odoo pricing plans … not available on One App Free or Standard plans" | [DOC] |
| Headers | [DOC]: `Host` required; `Authorization: bearer <key>` required; `Content-Type: application/json` required (charset recommended); `X-Odoo-Database` optional; `User-Agent` recommended | [DOC] |
| Bearer parsing | The regex is `^bearer\s+(.+)$`, case-insensitive. The key is checked with `res.users.apikeys._check_credentials(scope='rpc', …)`. An invalid key raises `Unauthorized("Invalid apikey")`, which is a 401 | [GH/odoo/addons/base/models/ir_http.py#L219-L240](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/ir_http.py#L219-L240) |
| Database header | If a session cookie and `X-Odoo-Database` both name different databases, the request fails with `Forbidden` ("Cannot use both the session_id cookie and the x-odoo-database header.") | [GH/odoo/http.py#L1904-L1914](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/http.py#L1904-L1914) |
| Body | [DOC]: "Post a JSON object". Keys are `ids` (array), `context` (object), and the method's parameters by name | [DOC]; signature at [json2.py#L57-L64](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L57-L64) |
| Argument binding | `inspect.signature(func).bind(records, **kwargs)`. A `TypeError` (missing or unknown argument) becomes **422** `UnprocessableEntity` | [json2.py#L80-L84](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L80-L84) |
| `ids` on model methods | Calling an `@api.model` method with non-empty `ids` returns **422** "cannot call <model>.<method> with ids" | [json2.py#L75-L77](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L75-L77) |
| Unknown model or method | **404** `NotFound` | [json2.py#L65-L74](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L65-L74) |
| Private methods | Methods whose name starts with `_`, or that are decorated `@api.private`, raise `AccessError` ("Private methods … cannot be called remotely"), which is a **403** | [GH/odoo/service/model.py#L45-L70](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/service/model.py#L45-L70) |
| Returned recordsets | A recordset result is converted to its `ids` list | [json2.py#L86-L88](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L86-L88) |
| Unparseable body | A JSON parse failure returns **400** "could not parse the body as json: …" | [GH/odoo/http.py#L2727-L2732](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/http.py#L2727-L2732) |
| Error body | `{name, message, arguments, context, debug}`. `debug` holds the full traceback | [GH/odoo/http.py#L468-L478](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/http.py#L468-L478); [DOC] |
| Error status mapping | `UserError` uses its `http_status`. `HTTPException` uses its own code. **Anything else (for example `ValueError`) returns 500** | [GH/odoo/http.py#L2746-L2768](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/http.py#L2746-L2768) |
| `http_status` values | `UserError`=422, `AccessDenied`=403, `AccessError`=403, `MissingError`=404, `LockError`=409. `ValidationError` subclasses `UserError`, so it is 422 | [GH/odoo/exceptions.py#L9-L119](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/exceptions.py#L9-L119) |
| Transactions | [DOC]: "All calls to the JSON-2 endpoint run in their own SQL transaction … committed in case of success and … discarded in case of error … not possible to chain multiple calls" | [DOC] (Transaction section) |
| Access rights | [DOC]: "All operations are validated against the access rights, record rules and field accesses of the user." It recommends dedicated bot users | [DOC] (Access Rights section) |
| API keys | [DOC]: a key created in the UI needs a description and a duration; keys cannot last more than three months. Programmatic `res.users.apikeys.generate()` checks expiry "against the maximum API key duration allowed by the roles assigned to the user" | [DOC] (API Keys section) |
| Rate limits, idempotency | [DOC] does not mention either | [DOC]: **UNVERIFIED** (the doc is silent) |

### 0a. Positional arguments (parent add 2)

The controller does not reject a positional (array) body with a specific line or message. Here is what the code does:

- `Json2Dispatcher.dispatch` sets `self.request.params = self.jsonrequest | args`. If the body is a JSON **array**, `list | dict` raises `TypeError`. The code catches it and sets `params = dict(args)`, so **the body is silently dropped** ([GH/odoo/http.py#L2733-L2736](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/http.py#L2733-L2736)).
- The method is then called with **no parameters**:
  - **Required parameter missing:** `search(domain, …)` requires `domain`, so `signature.bind` fails and the call returns **422** ([json2.py#L80-L84](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/json2.py#L80-L84)).
  - **All parameters optional:** every parameter of `search_read` is optional (`domain=None`), so the call runs with an **empty domain and all fields**, and returns every readable record ([GH/odoo/orm/models.py#L5767-L5795](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L5767-L5795)).
- This behaviour is derived from reading the code. It has not been observed live.

---

## O1. `search_read` / `search` / `read` arguments; `write_date` and `id` filter/order — **VERIFIED (code)**, live L1 pending

### Signatures (19.0)

| Method | Signature | Decorators | Source |
|---|---|---|---|
| `search` | `search(domain, offset=0, limit=None, order=None)`. `domain` is **required** | `@api.model`, `@api.readonly` | [GH/odoo/orm/models.py#L1374-L1391](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L1374-L1391) |
| `search_count` | `search_count(domain, limit=None)` | `@api.model`, `@api.readonly` | [models.py#L1358-L1360](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L1358-L1360) |
| `search_fetch` | `search_fetch(domain, field_names=None, offset=0, limit=None, order=None)` | `@api.model`, **`@api.private`**, so it **cannot be called over JSON-2** (403) | [models.py#L1393-L1403](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L1393-L1403) |
| `search_read` | `search_read(domain=None, fields=None, offset=0, limit=None, order=None, **read_kwargs)` | `@api.model`, `@api.readonly` | [models.py#L5765-L5806](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L5765-L5806) |
| `read` | `read(fields=None, load='_classic_read')`. A record method, so it takes `ids` in the body | `@api.readonly` | [models.py#L3474-L3496](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L3474-L3496) |

- `search_read` runs `search_fetch(domain or [], fields, offset=, limit=, order=)` and then `_read_format`, all **in one call** ([models.py#L5793-L5806](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L5793-L5806)).
- [DOC] gives the same guarantee: `search_read` does search and read in one transaction and cannot hit the missing-record race that `search` followed by `read` can.
- `fields` empty or omitted means all fields from `fields_get` ([models.py#L5793-L5794](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L5793-L5794)).
- `order` omitted means the model's `_order` is used ([models.py#L1421](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L1421)). `purchase.order._order = 'priority desc, id desc'` ([GH/addons/purchase/models/purchase_order.py#L26](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L26)).

### `write_date` and `id` exist on `purchase.order`

- `id` is in `MAGIC_COLUMNS = ['id'] + LOG_ACCESS_COLUMNS`, and `LOG_ACCESS_COLUMNS = ['create_uid','create_date','write_uid','write_date']` ([models.py#L296-L297](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L296-L297)).
- When `_log_access` is on (it defaults to `_auto`, which is true for `models.Model`), the metaclass adds `write_date = Datetime('Last Updated on', readonly=True)` as a **stored** field ([models.py#L283-L292](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L283-L292)).
- `purchase.order` is a `models.Model` and does not override `_log_access` ([purchase_order.py#L21-L26](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L21-L26)).
- Both are stored columns, so the domain operators `>`, `=` and `<` and `ORDER BY` apply. Domain grammar: `odoo/orm/domains.py`.

### `order` validation (parent add 2)

- **Syntax check:** `_check_qorder` matches the whole string against `regex_order` ([models.py#L93-L104](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L93-L104)). The regex allows `field[.property][:func] [asc|desc] [nulls first|nulls last]`, comma-separated.
  - On failure it raises `UserError('Invalid "order" specified (%s). A valid "order" specification is a comma-separated list of valid field names (optionally followed by asc/desc for the direction)')`. Over JSON-2 that is **HTTP 422** ([models.py#L5234-L5241](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L5234-L5241)).
- **Unknown field:** a field name that is syntactically valid but does not exist raises `ValueError(f"Invalid field {fname!r} on model {self._name!r}")` ([models.py#L5291-L5294](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L5291-L5294)).
  - `ValueError` is not a `UserError`, so `Json2Dispatcher.handle_error` returns **HTTP 500** with `name: "builtins.ValueError"` in the error body ([http.py#L2764-L2766](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/http.py#L2764-L2766)).
- **Malformed domain:** raises `ValueError("Domain() malformed domain …")` or `"invalid item in domain"`, which is also **500** ([GH/odoo/orm/domains.py#L257-L276](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/domains.py#L257-L276)).

### `write_date` precision (affects the §6a cursor)

- **Stored value:** written as `self.env.cr.now()` ([models.py#L4402](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L4402), [#L4542](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L4542)).
  - `cr.now()` is `SELECT (now() AT TIME ZONE 'UTC')`, which is **the transaction start time** ([GH/odoo/sql_db.py#L271-L278](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/sql_db.py#L271-L278)).
  - The column type is `timestamp` ([GH/odoo/orm/fields_temporal.py#L194](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L194)).
  - Whether sub-second digits survive storage is **UNVERIFIED**. `now()` carries microseconds and the write path does not strip them, but this was not confirmed live.
- **JSON output:** the JSON encoder calls `fields.Datetime.to_string`, which is `"%Y-%m-%d %H:%M:%S"`, **whole seconds, UTC** ([GH/odoo/tools/json.py#L62-L65](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/tools/json.py#L62-L65); [fields_temporal.py#L256-L265](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_temporal.py#L256-L265); format in [GH/odoo/tools/misc.py#L535-L537](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/tools/misc.py#L535-L537)).
- **Consequences for the plan's cursor** (`('write_date','>',ts) | (write_date = ts AND id > last_id)` with `ts` read back from JSON). These follow from the facts above and are not observed:
  - If stored values have sub-second digits, `write_date = ts` will not match rows inside that second, but `write_date > ts` will. Rows in the same second are therefore **re-delivered**, not lost.
  - `write_date` is the **transaction start**. A transaction that starts before the poll and commits after it gets an older `write_date` than the cursor and can be **skipped**. This is why the R3-§6a safety net exists.

### Operation: poll RFQs

| Item | Value |
|---|---|
| URL | `POST {base}/json/2/purchase.order/search_read` |
| Versions | Odoo 19.0 (JSON-2 is "New in version 19.0" per [DOC]) |
| Request | `{"domain":[…],"fields":[…],"order":"write_date asc, id asc","limit":100,"context":{…}}` |
| Response | 200 with an array of objects. Each object has `id` plus the requested fields. Many2one fields come back as `[id, display_name]` under the default `load='_classic_read'` (**UNVERIFIED** for 19.0: `_read_format` was not traced) |
| Auth | Bearer API key of the user. The user's ACLs, record rules and field access apply ([DOC]) |
| Errors | 401 invalid key; 403 access; 404 unknown model; 422 bad argument or bad `order` syntax; 500 unknown order field or malformed domain |
| Retry | Reads are idempotent. [DOC] gives no retry guidance |
| Limits | None documented |

---

## O2. `res.currency` `decimal_places` and `rounding` — **VERIFIED**

| Field | Definition | Source |
|---|---|---|
| `rounding` | `fields.Float('Rounding Factor', digits=(12, 6), default=0.01)`. The DB constraint `CHECK (rounding>0)` | [GH/odoo/addons/base/models/res_currency.py#L37-L38](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/res_currency.py#L37-L38), [#L53-L55](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/res_currency.py#L53-L55) |
| `decimal_places` | `fields.Integer(compute='_compute_decimal_places', store=True)` | [res_currency.py#L39-L40](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/res_currency.py#L39-L40) |
| Compute | If `0 < rounding < 1`, then `ceil(log10(1/rounding))`; otherwise `0` | [res_currency.py#L162-L168](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/res_currency.py#L162-L168) |
| `round()` | `tools.float_round(amount, precision_rounding=self.rounding)` | [res_currency.py#L216-L223](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/models/res_currency.py#L216-L223) |
| Tie rule | `float_round` defaults to `rounding_method='HALF-UP'`: "ties going away from zero". It applies a magnitude-scaled epsilon before `round()` | [GH/odoo/tools/float_utils.py#L72-L93](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/tools/float_utils.py#L72-L93), [#L104-L136](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/tools/float_utils.py#L104-L136) |
| Read access | `res.currency` is readable by `base.group_user`, `group_portal` and `group_public` (read=1); `group_system` has full access | [GH/odoo/addons/base/security/ir.model.access.csv#L59-L65](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L59-L65) |

- **Note:** `rounding` need not be a power of ten, for example `0.05`. In that case `decimal_places` (2) does not capture the rounding step. Converting to minor units with `decimal_places` alone loses the step. This follows from the compute and is not a documented warning.

---

## O3. Version and health probe — **VERIFIED (code)**

| Endpoint | Auth | Response | Source |
|---|---|---|---|
| `/web/version` or `/json/version` | `auth='none'`: no API key needed and none is checked. No `methods=` restriction | `{"version_info": [19,0,0,"final",0,""], "version": "19.0"}` (shape from `release.version_info` / `release.version`) | [GH/addons/rpc/controllers/__init__.py#L23-L29](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/rpc/controllers/__init__.py#L23-L29); [release.py#L15-L17](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/release.py#L15-L17) |
| `/web/health` | `auth='none'` | `{"status":"pass"}` with 200. With `?db_server_status=1` it adds `db_server_status`, and returns 500 plus `"fail"` if Postgres is unreachable | [GH/addons/web/controllers/home.py#L173-L188](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/web/controllers/home.py#L173-L188) |
| `/web/webclient/version_info` | `type='jsonrpc'`, `auth='none'` | not traced | [GH/addons/web/controllers/webclient.py#L87-L88](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/web/controllers/webclient.py#L87-L88) |

- **None of these checks the API key.** The cheapest **authenticated** probe is any JSON-2 call, for example `res.users/search_read` filtered to the key's own user, or the plan's `purchase.order/search_read limit 1`.
- A dedicated "whoami" JSON-2 method is **UNVERIFIED**: none was searched for or found.
- `version_info[3]` is the release-level constant `FINAL`. Its JSON string form (`"final"`) is **UNVERIFIED**: `RELEASE_LEVELS` values were not printed.
- The `/doc` page per database is mentioned in [DOC]. Its auth and content were not checked.

---

## O4. `picking_type_id` on `purchase.order` (`purchase_stock`) — **VERIFIED**

| Fact | Source |
|---|---|
| `purchase_stock` extends `purchase.order` with `picking_type_id = fields.Many2one('stock.picking.type', 'Deliver To', required=True, default=_default_picking_type, domain="['|', ('warehouse_id', '=', False), ('warehouse_id.company_id', '=', company_id)]")` | [GH/addons/purchase_stock/models/purchase_order.py#L13-L25](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L13-L25) |
| Default: `_get_picking_type(company_id)` returns the first `stock.picking.type` with `code='incoming'` in that company's warehouse, falling back to one with no warehouse, then to archived ones | [purchase_stock/models/purchase_order.py#L16-L18](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L16-L18), [#L346-L352](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L346-L352) |
| Destination location used by receipts: `picking_type_id.default_location_dest_id` (dropship and warehouse cases differ) | [purchase_stock/models/purchase_order.py#L329-L341](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase_stock/models/purchase_order.py#L329-L341) |

- The field exists **only when `purchase_stock` is installed**. Without it, `fields: ["picking_type_id"]` fails. The adapter should check field existence with `fields_get`.
  - The exact error raised for an unknown field in `fields` is **UNVERIFIED**. `read` documents `:raise ValueError: if a requested field does not exist` ([models.py#L3485](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/models.py#L3485)), which over JSON-2 would be 500.
- Reading `stock.picking.type` needs stock access rights. Which group the API user needs is **UNVERIFIED**: the `stock` ACLs were not read.

---

## O5. `res.company.po_double_validation` / `po_double_validation_amount` — **VERIFIED (code)**

| Fact | Source |
|---|---|
| `po_double_validation = fields.Selection([('one_step',…),('two_step',…)], default='one_step')`, with **no `groups=`** attribute | [GH/addons/purchase/models/res_company.py#L16-L20](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/res_company.py#L16-L20) |
| `po_double_validation_amount = fields.Monetary('Double validation amount', default=5000)`, with **no `groups=`**. It is a Monetary on `res.company`. Its currency field was not traced; the expected default is `currency_id` (**UNVERIFIED**) | [res_company.py#L22-L23](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/res_company.py#L22-L23) |
| ACL: `res.company` is readable by `base.group_user`, `group_portal` and `group_public`; `group_erp_manager` has full access | [GH/odoo/addons/base/security/ir.model.access.csv#L42-L45](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/ir.model.access.csv#L42-L45) |
| Record rules: employee, portal and public users see only `[('id','in', company_ids)]`; `group_erp_manager` sees all | [GH/odoo/addons/base/security/base_security.xml#L104-L132](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/addons/base/security/base_security.xml#L104-L132) |
| How it is used: `_approval_allowed()` returns true when the policy is `one_step`, or when it is `two_step` and `amount_total < po_double_validation_amount` (converted from **`self.env.company.currency_id`** into the order currency at `date_order`), or when the user has `purchase.group_purchase_manager` | [GH/addons/purchase/models/purchase_order.py#L1257-L1266](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L1257-L1266) |
| Settings mirror: `res.config.settings.po_order_approval` / `po_double_validation(_amount)` are `related='company_id.…'` | [GH/addons/purchase/models/res_config_settings.py#L11-L15](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/res_config_settings.py#L11-L15) |

- **Rights needed:** an internal user (`base.group_user`) whose `company_ids` include the PO's company can `read` both fields through JSON-2. No purchase-specific group is needed to read them.
- **Code-level note:** the threshold conversion uses `self.env.company.currency_id`, the *current* company's currency, as the source currency. The amount field itself belongs to `self.company_id`. Whether that difference matters for a single-company API user is **UNVERIFIED**.

---

## O6. How `price_subtotal` and `amount_untaxed` are computed — **VERIFIED (code)** for untaxed and price-excluded cases; **PARTIAL** for price-included taxes

### Inputs and their precision

| Input | Definition | Precision effect | Source |
|---|---|---|---|
| `price_unit` | `fields.Float('Unit Price', required=True, min_display_digits='Product Price', …)` | `min_display_digits` with no `digits` sets `digits=False`. The column is then `numeric` with **no fixed scale**, and **no rounding is applied** on cache or write. "Product Price" affects **display only** | [GH/addons/purchase/models/purchase_order_line.py#L46-L48](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L46-L48); [GH/odoo/orm/fields_numeric.py#L69-L75](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_numeric.py#L69-L75), [#L121-L133](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_numeric.py#L121-L133), [#L156-L170](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_numeric.py#L156-L170) |
| "Product Price" precision record | `decimal.precision` "Product Price", default `digits=2`. Since `price_unit` no longer uses it for storage, it plays **no part in the subtotal maths** in 19.0 | [GH/addons/product/data/product_data.xml#L19-L22](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/product/data/product_data.xml#L19-L22) |
| `discount` | `fields.Float('Discount (%)', min_display_digits='Discount')`. Same rule: not rounded | [purchase_order_line.py#L29-L33](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L29-L33) |
| `product_qty` | `fields.Float('Quantity', digits='Product Unit', required=True)`. **Rounded** to the "Product Unit" precision when cached and written. That precision's default value is **UNVERIFIED** (data file not read) | [purchase_order_line.py#L23](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L23); [fields_numeric.py#L166-L170](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_numeric.py#L166-L170) |
| `price_subtotal`, `price_total` | `fields.Monetary(compute='_compute_amount', store=True)` | Monetary column: `float_repr(currency.round(value), currency.decimal_places)` | [purchase_order_line.py#L54-L56](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L54-L56); [fields_numeric.py#L264](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/orm/fields_numeric.py#L264) |
| `tax_calculation_rounding_method` | On **`res.company`** (account addon): `Selection([('round_globally','Round per Tax'),('round_per_line','Round per Line')], default='round_globally')` | **Default is `round_globally`** | [GH/addons/account/models/company.py#L129-L132](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/company.py#L129-L132) |

### Line: `price_subtotal`

Code at [purchase_order_line.py#L123-L133](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L123-L133):

```python
base_line = line._prepare_base_line_for_taxes_computation()
AccountTax._add_tax_details_in_base_line(base_line, company)
AccountTax._round_base_lines_tax_details([base_line], company)
line.price_subtotal = base_line['tax_details']['total_excluded_currency']
```

1. **Base line.** `price_unit`, `quantity=product_qty`, `discount`, `tax_ids`, `currency_id = order.currency_id or company.currency_id`, `rate = order.currency_rate`.
   - Sources: [purchase_order_line.py#L135-L151](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order_line.py#L135-L151); loader at [GH/addons/account/models/account_tax.py#L1651-L1671](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1651-L1671).
2. **Tax details.** `rounding_method = company.tax_calculation_rounding_method` and `price_unit_after_discount = price_unit * (1 - discount/100)`. Then `tax_ids._get_tax_details(price_unit_after_discount, quantity, precision_rounding=currency.rounding, rounding_method)` ([account_tax.py#L1787-L1798](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1787-L1798)). Inside it:
   - `raw_base = quantity * price_unit`, and **only for `round_per_line`** it is rounded: `float_round(raw_base, precision_rounding=currency.rounding)` ([account_tax.py#L1259-L1261](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1259-L1261)).
   - Each tax amount is rounded per tax **only for `round_per_line`** ([account_tax.py#L1204-L1207](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1204-L1207)).
   - **No taxes (`tax_ids` empty):** `total_excluded = total_included = raw_base` ([account_tax.py#L1336-L1341](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1336-L1341)).
   - With taxes: `total_excluded = taxes_data[0]['base']`. For price-included taxes the base is `raw_base − Σ batch tax` ([account_tax.py#L1309-L1312](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1309-L1312), [#L1336-L1339](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1336-L1339)).
   - The result is stored as `raw_total_excluded_currency` ([account_tax.py#L1811-L1817](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L1811-L1817)).
3. **Rounding.** `_round_base_lines_tax_details` sets `total_excluded_currency = base_line['currency_id'].round(raw_total_excluded_currency)` ([account_tax.py#L2268-L2279](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2268-L2279)).
   - It also computes `delta_total_excluded_currency` (global-rounding adjustments, [#L2306-L2319](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2306-L2319)), but **`price_subtotal` reads `total_excluded_currency` only and ignores the delta**.

**Rule for an untaxed line (`tax_ids` empty), both rounding methods:**

```
price_subtotal = currency.round( price_unit × (1 − discount/100) × product_qty )
```

- Currency rounding uses `float_round(..., precision_rounding=currency.rounding)` with HALF-UP (see O2).
- The multiplication is done in Python `float`. Under `round_per_line` the value is rounded once in `_get_tax_details` and again by `currency.round`, which gives the same result.
- **The precision is the order currency's `rounding`, not "Product Price".** "Product Price" never enters the calculation.

**Price-excluded taxes:** the line's `total_excluded` is the base, `raw_base` (rounded first under `round_per_line`). `price_subtotal` is the same expression as the untaxed case.

**Price-included taxes:** the base is `raw_base − tax`, and tax amounts depend on the tax configuration (`_eval_tax_amount_price_included`). **PARTIAL:** the per-tax formulas were not traced.

### Order: `amount_untaxed`

Code at [GH/addons/purchase/models/purchase_order.py#L28-L44](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/purchase/models/purchase_order.py#L28-L44):

```python
order_lines = order.order_line.filtered(lambda x: not x.display_type)
base_lines = [line._prepare_base_line_for_taxes_computation() for line in order_lines]
AccountTax._add_tax_details_in_base_lines(base_lines, order.company_id)
AccountTax._round_base_lines_tax_details(base_lines, order.company_id)
tax_totals = AccountTax._get_tax_totals_summary(base_lines=base_lines, currency=order.currency_id or order.company_id.currency_id, company=order.company_id)
order.amount_untaxed = tax_totals['base_amount_currency']
```

- **Section and note lines** (`display_type`) are excluded.
- **Summary.** `base_amount_currency += values['total_excluded_currency']` over the aggregated groups ([account_tax.py#L2825-L2836](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2825-L2836)). The aggregation **includes the delta**: `excluded_rounded_amount = tax_details['total_excluded'] + tax_details['delta_total_excluded']` per suffix ([account_tax.py#L2611-L2622](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2611-L2622)).
- **Where the delta comes from.** `_round_tax_details_base_lines` groups lines by `(currency, is_refund, computation_key)` ([#L2052-L2057](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2052-L2057)). In "excluded" mode it sets `delta = currency.round(Σ raw_total_excluded) − Σ total_excluded` and spreads the delta over the lines ([#L2082-L2128](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2082-L2128)).
  - Docstring example: two lines of 12.12 × 12.12 give a group total of 293.79, against a line sum of 146.89 + 146.89 = 293.78 ([#L2036-L2040](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2036-L2040)).
- **Results, untaxed or price-excluded lines, one currency** (derived from the code above):
  - **`round_per_line`:** each `raw_total_excluded` is already rounded, so the delta is 0 and **`amount_untaxed = Σ price_subtotal`**.
  - **`round_globally` (the default):** **`amount_untaxed = currency.round( Σ_lines price_unit × (1−disc/100) × qty )`**, which **can differ from `Σ price_subtotal`** by a few minor units. NetSuite-side totals (V2 in PLAN R3-§2b) must be compared against this rule, not against the sum of the lines.
- **Price-included taxes** ("included" or "mixed" mode, [#L2047-L2075](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/addons/account/models/account_tax.py#L2047-L2075)): **PARTIAL**, not traced to a closed formula.
- **No sandbox run.** These rules come from reading code only. Golden values for T30 should be confirmed on live instance L1.

---

## UNVERIFIED (Odoo)

1. Rate limits and idempotency for JSON-2: [DOC] is silent.
2. Live behaviour of everything above. No Custom-plan Odoo 19 instance is available (gate L1).
3. Whether `write_date` keeps sub-second digits in storage. The JSON output is whole seconds (verified).
4. The many2one output shape (`[id, name]`) under `load='_classic_read'` in 19.0 `_read_format`.
5. The JSON string form of `version_info[3]` (`FINAL`) from `/web/version`.
6. Any key-authenticated "whoami" or version method on JSON-2.
7. The exact error for an unknown name in `fields`. It is documented as `ValueError`, which would be HTTP 500 per the dispatcher.
8. The `stock.picking.type` read group the API user needs.
9. The currency field of `res.company.po_double_validation_amount`, and the effect of `_approval_allowed` using `env.company` currency in multi-company setups.
10. The default digits of the "Product Unit" precision, which `product_qty` is rounded to.
11. Closed-form `price_subtotal` and `amount_untaxed` with **price-included** taxes or fixed or group taxes.
12. Exact parity with Odoo's float maths.
    - **Verified:** `float_round` divides by the rounding step (inverting it when the step is below 1), then for HALF-UP adds an epsilon of `copysign(2**(log2(|x|)-50), x)` before Python `round()`. The code comment says the epsilon is there "to tip the tie-break in the right direction" ([GH/odoo/tools/float_utils.py#L104-L136](https://github.com/odoo/odoo/blob/61d8ae93ae5d2af016a794f67df3f1bb3df1d94f/odoo/tools/float_utils.py#L104-L136)).
    - **Not verified:** that integer or decimal arithmetic in our code reproduces Odoo's float result in every case.
