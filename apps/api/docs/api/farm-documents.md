# Plot title documents

Photos or PDFs of a plot's title deed or other land papers, filed at registration so the verifier can check the plot against them. Code: `src/modules/farm_documents/`, migration `farm_plot_documents`.

## Who

- Field staff of the plot's village (field officers there, ops, admin) list, read and file documents.
- **Farmers see nothing**, their own plot's documents included: `404`. So do officers of other villages and other workspaces. No token: `401`.

## Append-only

- A filed document is never renamed, replaced or deleted. There is no `PUT`, `PATCH` or `DELETE` route, and a trigger refuses updates and deletes in the database (`plot documents cannot be changed`).
- A wrong upload is followed by a right one.
- Documents live in their own table, not the workspace `files` table, so the file list and `DELETE /files/{id}` never reach them.
- The storage key is `genesy/{workspace}/farm/{village}/{plot}/{id}-{attempt}`. The table CHECK `farm_plot_documents_key_in_plot` ties it to its plot's folder, and the download route checks the prefix again before signing.

## `POST /v1/workspace/{workspace}/farm/plots/{plot}/documents`

A multipart form with two fields:

| Field | Rule |
|---|---|
| `file` | required; JPEG, PNG, WebP, HEIC or PDF, at most 10 MB |
| `id` | optional uuid made on the device. A retry with the same id returns the document already filed (`200`) and stores nothing |

- The type is read from the bytes. The file name and the part's `Content-Type` are ignored, so a PNG named `deed.pdf` is stored as `image/png`.
- Anything else is `422`: another type, an empty file, an unknown form field (e.g. `uploaded_by`), or a non-multipart body.
- `uploaded_by` is always the caller.
- Each attempt writes its own object, so two racing retries never overwrite each other. The loser's object is deleted.
- Replies:
  - `201 {"data": {id, village_id, plot_id, file_name, content_type, size_bytes, uploaded_by {id, name}, uploaded_at}}`.
  - `409` when the id belongs to another plot's document.
  - `404` outside the caller's villages, checked before the body is read.
  - `503` without a bucket.

## `GET /v1/workspace/{workspace}/farm/plots/{plot}/documents`

`200`: one page, oldest first, `?page&page_size`.

## `GET /v1/workspace/{workspace}/farm/plots/{plot}/documents/{document}`

`200`: the document plus `download_url` and `download_expires_at`. The link is presigned for 10 minutes, under the 15-minute ceiling. PDFs and photos open inline.
