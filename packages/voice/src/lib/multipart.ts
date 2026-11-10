/** Browser transport only: business uploads use original session bytes, never provider IDs. */
export function batchAttachmentReferences(calls: unknown) {
  if (!Array.isArray(calls)) return undefined
  const references: Record<string, string[]> = Object.create(null)
  let found = false
  for (const call of calls) {
    const attachments = call?.arguments?.attachments
    if (attachments === undefined) continue
    if (
      !attachments ||
      typeof attachments !== "object" ||
      Array.isArray(attachments)
    )
      throw new Error("invalid_references")
    for (const [field, group] of Object.entries(attachments)) {
      if (
        !Array.isArray(group) ||
        group.some(
          (handle) =>
            typeof handle !== "string" || !handle || handle === "payload"
        )
      )
        throw new Error("invalid_references")
      references[field] = [...(references[field] ?? []), ...group]
    }
    found = true
  }
  return found ? references : undefined
}

export function voiceRequestBody(
  payload: Record<string, unknown>,
  references: unknown,
  originals: Map<string, Blob>
): { body: string | FormData; headers: Record<string, string> } {
  if (references === undefined)
    return {
      body: JSON.stringify(payload),
      headers: { "Content-Type": "application/json" },
    }
  if (
    !references ||
    typeof references !== "object" ||
    Array.isArray(references)
  )
    throw new Error("invalid_references")
  const handles = new Set<string>()
  for (const group of Object.values(references)) {
    if (
      !Array.isArray(group) ||
      group.some(
        (handle) =>
          typeof handle !== "string" || !handle || handle === "payload"
      )
    )
      throw new Error("invalid_references")
    for (const handle of group as string[]) handles.add(handle)
  }
  const body = new FormData()
  body.append("payload", JSON.stringify(payload))
  for (const handle of handles) {
    const original = originals.get(handle)
    if (!original) throw new Error("missing_reference")
    body.append(
      handle,
      original,
      original instanceof File ? original.name : "reference"
    )
  }
  return { body, headers: {} }
}
