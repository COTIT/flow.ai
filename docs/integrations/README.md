# Verzo integrations (WF-01)

WF-01: an Odoo 19 draft RFQ goes to a Flow approval, then exactly one NetSuite `purchaseOrder` is created.

- Plan: [PLAN.md](PLAN.md)
- Contracts: [contracts.md](contracts.md)
- Facts packs (documented contract, not observed behaviour): [facts/odoo-19.md](facts/odoo-19.md), [facts/netsuite-rest.md](facts/netsuite-rest.md)
- Sandbox setup and live smoke kit: [SANDBOX.md](SANDBOX.md)

Wording rule (PLAN §8): until every gate below is passed, say "verified against documented contract", never "integrated with Odoo/NetSuite".

## LIVE gates

A gate passes only when its live test runs green against a real sandbox and its observed blocks are copied into the facts pack. L4 is mandatory for any production claim.

| Gate | What it proves | Test (`apps/api/src/integrations/live.rs`) | Status | Run date | Facts updated |
|---|---|---|---|---|---|
| L1 | Odoo end-to-end read on a real Custom-plan v19 instance | `live_l1_odoo_read` | open | — | — |
| L2 | NetSuite create / duplicate externalId behaviour | `live_l2_netsuite_create_and_duplicate_external_id` | open | — | — |
| L3 | Required purchaseOrder fields (location/subsidiary) under a real account's features | `live_l3_netsuite_required_fields` | open | — | — |
| L4 | Approval routing (mandatory) | `live_l4_netsuite_approval_routing` | open | — | — |
| L5 | M2M token + permissions on a least-privilege role | `live_l5_netsuite_m2m_least_privilege` | open | — | — |
| L6 | Currency/vendor restrictions | `live_l6_netsuite_currency_vendor_restriction` | open | — | — |
