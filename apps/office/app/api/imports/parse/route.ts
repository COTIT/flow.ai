import "server-only"
import {
  readSheet,
  SHEET_LIMIT,
  SheetError,
  sheetType,
} from "@/lib/imports/sheet"

export const runtime = "nodejs"

/** Room for the multipart envelope around a 2 MiB file. */
const FORM_LIMIT = SHEET_LIMIT + 64 * 1024

const refuse = (message: string, status: number) =>
  Response.json(
    { message },
    { status, headers: { "Cache-Control": "no-store" } }
  )

/** Plain member-login check: the bearer must list this workspace. No voice session, no writes. */
async function requireMember(request: Request, workspaceId: string | null) {
  const auth = request.headers.get("authorization") ?? ""
  if (!/^Bearer \S+$/.test(auth))
    throw new SheetError("Sign in to import a file.", 401)
  if (!workspaceId || !/^[a-zA-Z0-9-]{1,100}$/.test(workspaceId))
    throw new SheetError("Choose a workspace.", 422)
  const apiUrl = process.env.API_URL ?? "http://localhost:8080/v1"
  const res = await fetch(`${apiUrl}/me/workspaces`, {
    headers: { Authorization: auth, Accept: "application/json" },
    cache: "no-store",
    redirect: "error",
    signal: AbortSignal.timeout(15000),
  })
  if (!res.ok)
    throw new SheetError(
      "Your session could not be verified. Sign in again.",
      res.status === 401 ? 401 : 403
    )
  const body = (await res.json()) as { data?: { id: string }[] }
  if (!body.data?.some((w) => w.id === workspaceId))
    throw new SheetError("You do not have access to this workspace.", 403)
}

/** Parse an uploaded .csv/.xlsx into columns and rows for the import mapping step. */
export async function POST(request: Request) {
  try {
    await requireMember(request, request.headers.get("x-workspace-id"))
    const declared = Number(request.headers.get("content-length") ?? NaN)
    if (Number.isFinite(declared) && declared > FORM_LIMIT)
      throw new SheetError("Spreadsheets must not exceed 2 MiB.", 413)
    let form: FormData
    try {
      form = await request.formData()
    } catch {
      throw new SheetError("Send the file as a form upload.", 422)
    }
    const file = form.get("file")
    if (!(file instanceof Blob))
      throw new SheetError("Choose a .csv or .xlsx file.", 422)
    if (file.size > SHEET_LIMIT)
      throw new SheetError("Spreadsheets must not exceed 2 MiB.", 413)
    const bytes = new Uint8Array(await file.arrayBuffer())
    const sheet = readSheet(bytes, sheetType(bytes, file.type))
    return Response.json(
      { data: sheet },
      { headers: { "Cache-Control": "no-store" } }
    )
  } catch (error) {
    if (error instanceof SheetError) return refuse(error.message, error.status)
    return refuse("This file could not be read. Try again.", 500)
  }
}
