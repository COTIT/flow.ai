# Files API

Workspace files (uploads, logos, generated PDFs). Ported from vimdesk-api `src/Domain/System/File` (StoreFileRequest, CreateFile, FileResource). The bytes live in the S3-compatible bucket Laravel already uses (Backblaze B2); Postgres keeps only the metadata row.

Shared rules (guard order, 404s, list `meta`) are in `conventions.md`. Every route sits under `/v1/workspace/{workspace}`.

## Routes

| Route | Permission | Answer |
|---|---|---|
| `GET /files` | `file.list` | paginated `FileResource` rows, newest first |
| `GET /files/{id}` | `file.get` | `{data: FileResource}` |
| `POST /files` | `file.create` | `201 {"data": {"file_id": "<uuid>"}}` |
| `DELETE /files/{id}` | `file.delete` | `{"message": "ok"}` |

- An unknown, malformed, deleted or other workspace's id is a 404.
- Without storage configured every route answers `503 {"message": "File storage isn't configured."}`.

## Upload `POST /files`

JSON body, up to 14 MiB:

| Field | Rule |
|---|---|
| `upload` | required; the file as a base64 string (strict alphabet, no `data:` prefix). A JSON string is refused. Error: `The upload must be a file or a base64 string.` |
| `document_name` | required string ≤ 255; its extension sets the stored extension and content type |
| `description` | string ≤ 255 |
| `public` | required boolean (accepted, not used: every object is private) |
| `meta` | object, ≤ 1000 JSON nodes / 16 KiB |

- The decoded file may be at most 10 MB: `The upload must not be greater than 10240 kilobytes.`
- One upload per workspace at a time (shared with PDF renders), and at most 4 across the server. Both are taken after the permission check and before the body is read; a request waits up to 10 s for each, then gets a 503 with `Retry-After`.
- When the bucket fails or is down: `503 {"message": "File storage is unavailable. Try again later."}`.
- Multipart uploads (Laravel's `upload` file part) are not ported yet: the SPA sends base64.

### How a file is stored

1. A `pending` row is committed: id (uuidv7), key `genesy/{workspace_id}/{id}.{ext}`, size, sha256, content type.
2. The object is written to the bucket.
3. The row becomes `stored`.

If step 2 or 3 fails, the row stays `pending`. An hourly sweep claims rows pending for over an hour as `deleting`, removes their object (a missing one is fine) and marks them `deleted`; an upload that lands after its row was claimed fails instead of marking it stored. The bucket is never listed, so objects other apps keep there are never touched. Rows are never hard-deleted.

A delete marks the row `deleting`, removes the object, then marks it `deleted`. A delete that fails part way is finished by the next sweep once the row is 5 minutes old. A file a PDF template uses can be deleted; the template then prints without it (as in Laravel, whose files were soft-deleted too).

## FileResource

```json
{
  "id": "<uuid>",
  "file_path": "<signed URL, valid 10 minutes>",
  "size": 19,
  "extension": "pdf",
  "document_name": "Invoice 1.pdf",
  "created_at": "…", "last_updated_at": "…",
  "association_data": [],
  "meta": null,
  "relationships": { "uploaded_by": { "id": "…", "name": "…", "initials": "…", "email": "…", "avatar_path": "…", "color": null } }
}
```

- `file_path` is a presigned GET URL on the bucket, valid for 10 minutes. PDFs and raster images (png, jpeg, gif, webp) open inline; any other type, SVG included, downloads (`Content-Disposition: attachment`), so a stored HTML or SVG file can't run script in the browser.
- `uploaded_by` is null once that user has left the workspace.

## Laravel differences

- Bytes go to the bucket under `genesy/{workspace}/`; Laravel kept a disk path. The public `/storage/files/...` routes are not ported: downloads use the signed URL.
- Laravel took the MIME type from the bytes (`finfo`); here it comes from the extension of `document_name`, and only known types are served inline.
- `association_data` is always `[]`: file associations aren't ported.
