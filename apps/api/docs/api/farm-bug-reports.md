# Farm bug reports

The in-app bug button (RUAHA `bug_report`): a screenshot of the page (or a photo when that fails), the last 30 things the user did, and what they say went wrong. Code: `src/modules/farm_bug_reports/`, migration `farm_loans_bug_reports`.

- Anyone with a farm role files, always under their own name.
- Only farm admin reads. Not even the reporter reads a report back; ops, officers and farmers get `403`.
- No route changes or deletes a report. `status` (`open`, `in_progress`, `fixed`, `wont_fix`) and `resolution` move through operator SQL only.

## `POST /v1/workspace/{workspace}/farm/bug-reports` (multipart)

Fields: `route`, `user_agent`, `viewport`, `description` (not blank), `actions` (a JSON array, at most 30), `screenshot` and/or `photo` (at least one; JPEG, PNG, WebP or HEIC by their bytes, 5 MB each), optional `id` (send the same one on a retry). Any other field is `422 {"message": "The upload has an unknown field."}`, so a client cannot name a reporter or a storage key. `201 {"data": {"id": "…", "ref": 12}}`; a retried `id` of the same reporter is `200` with the same body, someone else's `409`.

Images are stored at `genesy/{workspace}/farm/bug-reports/{reporter}/{id}-{uuid}.{ext}`: a fresh key per upload, so nothing is ever replaced.

## `GET /v1/workspace/{workspace}/farm/bug-reports?page&page_size`

Farm admin. Newest first: `{id, ref, reporter {id, name}, route, user_agent, viewport, description, actions, has_screenshot, has_photo, screenshot_url: null, photo_url: null, status, resolution, created_at}`.

## `GET /v1/workspace/{workspace}/farm/bug-reports/{report}`

Farm admin. The same, with `screenshot_url` / `photo_url` signed for 10 minutes.
