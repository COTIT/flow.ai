/** New shares always name their workspace's active connection; never use the legacy root. */
export function storefrontUrl(
  base: string | undefined,
  connection: { id: string | null; active: boolean } | null | undefined,
  publicCode?: string
): string | null {
  if (
    !base ||
    !connection?.active ||
    !connection.id ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
      connection.id
    )
  )
    return null
  try {
    const url = new URL(base)
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    )
      return null
    url.pathname = `${url.pathname.replace(/\/$/, "")}/s/${connection.id}${publicCode ? `/b/${encodeURIComponent(publicCode)}` : ""}`
    return url.href
  } catch {
    return null
  }
}
