# Scheduled reports

A member's own read-only reports, mailed to their own login email on a schedule. Created by the office voice assistant (normal review card); listed, paused and deleted in office **Settings → Scheduled reports**. A schedule names one fixed report, never an operation, so nothing can write on a schedule.

All member routes need `Authorization: Bearer <token>` and an active membership of `{workspace}`. Each member sees and changes only their own rows: another member's or another workspace's schedule is 404.

## Reports

| `report` | Content | Needs | Sent when |
|---|---|---|---|
| `sales_summary` | Orders sold and totals incl VAT per currency (same sold rule as `orders_sales_summary`) for the day before (daily), the 7 days before (weekly) or the calendar month before (monthly), ending at local midnight | `order.list` | always |
| `attention` | Today's to-do counts (`attention_summary`); sections the reader can't list are left out | member | always |
| `overdue_invoices` | Overdue count and the oldest overdue invoices with open money | `invoice.list` | something is overdue |
| `stock` | Products sold out or at/below `product.low_stock_threshold` (default 5), lowest first, at most 20 | `stock.list` | something is low |

Per-product thresholds ("eggs below 10 trays") are not supported; `stock` uses the workspace threshold.

## Timing

`frequency` `daily`, `weekly` (`weekday` 1 = Monday … 7 = Sunday) or `monthly` (`day_of_month` 1-28), at local `hour` 0-23 in the workspace `general.timezone` (UTC when unset or unknown). The slot is computed in Postgres, so DST shifts follow the zone. The job runs every 5 minutes, so mail arrives within about 5 minutes after the hour.

## GET /v1/workspace/{workspace}/scheduled-reports

200 `{data: [ScheduledReport]}`, the caller's own, oldest first (at most 10).

`ScheduledReport`: `id`, `report`, `frequency`, `weekday` (null unless weekly), `day_of_month` (null unless monthly), `hour`, `time_zone`, `active`, `paused_reason` (`user` | `access_lost` | `unsubscribed` | null), `next_run_at`, `email` (the only recipient), `last_run_at`, `last_outcome` (`sent` | `skipped_empty` | `paused_access` | `failed` | null), `created_at`.

## POST /v1/workspace/{workspace}/scheduled-reports

Body `{report, frequency, weekday?, day_of_month?, hour}`. No recipient field.

- 201 `{data: ScheduledReport}`; `next_run_at` is the first slot after now.
- 403 without the report's permission (table above).
- 422: `report`/`frequency` missing or unknown; `hour` missing or outside 0..23; `weekday` missing for weekly or sent for another frequency (`The weekday field does not apply to this frequency.`); same for `day_of_month` with monthly; 10 schedules already (`report`: `You can keep at most 10 scheduled reports in this workspace.`).

## POST /v1/workspace/{workspace}/scheduled-reports/{schedule}

Body `{active: bool}`. `false` pauses (`paused_reason` `user`). `true` resumes from the next slot after now and rechecks the report's permission (403 without it). 200 `{data: ScheduledReport}`; 404 for a schedule that isn't the caller's; 422 when `active` is missing or not a boolean.

## DELETE /v1/workspace/{workspace}/scheduled-reports/{schedule}

200 `{"message": "ok"}`; 404 for a schedule that isn't the caller's. The run log goes with it.

## POST /v1/scheduled-reports/unsubscribe (public, rate limited)

Body `{token}` from the mail's link (`{FRONTEND_URL}/unsubscribe?token=…`; the office page posts it after a click, so mail scanners opening the link stop nothing). Pauses that one schedule (`unsubscribed`; an already paused one keeps its reason). Again is fine. 200 `{data: {report}}`; 404 unknown or expired link (run log older than 90 days); 422 `token` missing.

## Runs (job `scheduled_reports`, every 5 minutes)

1. Claim due active schedules (`FOR UPDATE SKIP LOCKED`, 20 per transaction; no new batch after 8 s, the rest wait for the next tick).
2. Recheck the owner: active member of the workspace and holding the report's permission. Otherwise the schedule pauses (`access_lost`), the run is logged `paused_access`, nothing is sent.
3. Move `next_run_at` to the next slot after now and commit before any mail leaves. A crash after that loses one mail and never sends twice; an outage sends once, not a backlog.
4. Build the report with the owner's permissions; `overdue_invoices`/`stock` with nothing to say log `skipped_empty`.
5. Mail the owner's login email (English plain text for now) with the manage link (`{FRONTEND_URL}/settings/scheduled-reports`) and a fresh unsubscribe token; only its sha256 is stored, on the run row. Logged `sent`, or `failed` when the send fails.

Run rows older than 90 days are pruned by the hourly `prune` job.
