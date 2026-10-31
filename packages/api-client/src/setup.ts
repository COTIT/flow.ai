import { configureApi } from "./mutator"
import { sessionStore } from "./session"

/**
 * Browser side of every app: calls go to same-origin `/v1` (each app's next.config
 * forwards it to the backend, which sends no CORS headers), with the session token,
 * and a 401 logs out. Server code configures its own client and passes headers per call.
 */
export function connectBrowser() {
  return configureApi({
    baseURL: "/v1",
    getToken: () => sessionStore.getState().token,
    onUnauthorized: () => sessionStore.getState().signOut(),
  })
}
