/** Calls a voice BFF route under `apiBase` and reads its JSON; a 401 tells the host the session expired. */
export async function voiceFetch(
  apiBase: string,
  path: string,
  init: RequestInit,
  onUnauthorized: () => void
) {
  const response = await fetch(`${apiBase}/${path}`, init)
  const value = await response.json()
  if (response.status === 401) onUnauthorized()
  return { response, value }
}
