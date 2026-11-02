# Farm demo seed

RUAHA's data was demo only (G15), so nothing is migrated. A command builds the demo story in a workspace called `ruaha-demo`:

```bash
genesy-360-api seed-farm-demo   # or: cargo run -- seed-farm-demo
```

- **CLI only.** No HTTP route seeds anything. The command runs migrations, seeds, prints the demo accounts and their passwords once, and exits without binding a port.
- **Runs once.** If a workspace named `ruaha-demo` exists and is flagged as demo (`meta.demo = true`), it does nothing. If one exists without the flag, it refuses and exits 1: a real customer's workspace is never touched.
- **Through the real rules.** Every step goes through the API's own routes in-process, so provenance, four-eyes checks, audit rows and voucher events are exactly what live use would write. Staff join the workspace by direct insert, so no invite mail is sent.

The story (code: `src/farm_demo.rs`, test: `tests/api/farm_demo.rs`):
- **Villages:** Ilundo and Mgama.
- **Crops:** maize (by area), banana and coffee (by tree count).
- **Staff:** Amina (admin, workspace owner), Asha (ops), Salima and Juma (Ilundo officers), Peter (Mgama officer).
- **Registration:** Salima registers the Mwakalinga household (Neema, then Rehema joining her household and farm, with maize and banana intercropped on one plot), the Kimaro household (Asha) and the Mfaume household (Bakari). Peter registers Joseph Mlowe in Mgama. Neema's maize estimate is revised from 3,200 to 4,100 kg; the first figure stays on record.
- **Verification:** Asha (ops) verifies the three Ilundo households, a second pair of eyes after the registrar.
- **Logins:** Neema, Asha Kimaro and Bakari get logins from Salima and choose their own password.
- **Surveys:** five TZS surveys. Three households answer one each, worth TZS 5,000, 3,000 and 2,000, and those surveys close. Juma redeems Bakari's TZS 2,000.
- **End state:** TZS 10,000 issued, 2,000 redeemed, 8,000 outstanding. Neema still has two surveys to answer: Maize storage and Harvest labour.

The demo passwords are random on every run and printed once, to stdout, by the command. They are never stored in plain text or logged; a rerun on an existing demo prints none.
