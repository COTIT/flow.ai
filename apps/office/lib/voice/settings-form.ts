import "server-only"

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

function prefill(current: unknown, changed: unknown): unknown {
  if (!object(changed)) return changed
  const existing = object(current) ? current : {}
  return {
    ...existing,
    ...Object.fromEntries(
      Object.entries(changed).map(([key, value]) => [
        key,
        prefill(existing[key], value),
      ])
    ),
  }
}

/** Native form prefills masked values; Rust keeps credential binding and validation authority. */
export function settingsFormValues(
  current: unknown,
  changed: unknown
): Record<string, unknown> {
  if (!object(current) || !object(changed))
    throw new Error("Settings form values are unavailable")
  return Object.fromEntries(
    Object.entries(changed).map(([section, value]) => [
      section,
      prefill(current[section], value),
    ])
  )
}
