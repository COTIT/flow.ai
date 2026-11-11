import { VoiceRequestError } from "./errors"

/** Calls a voice BFF route under `apiBase` and reads its JSON; a 401 tells the host the session expired. */
export async function voiceFetch(
  apiBase: string,
  path: string,
  init: RequestInit,
  onUnauthorized: () => void
) {
  let response: Response
  try {
    response = await fetch(`${apiBase}/${path}`, init)
  } catch {
    const kind = globalThis.navigator?.onLine === false ? "offline" : "network"
    throw new VoiceRequestError(kind, 0, undefined, kind)
  }
  let value
  try {
    value = await response.json()
  } catch {
    // An error page (a proxy's HTML 502) keeps its status; an unreadable success is not trusted.
    if (response.ok)
      throw new VoiceRequestError("bad_response", 0, undefined, "bad_response")
  }
  if (response.status === 401) onUnauthorized()
  return { response, value }
}
