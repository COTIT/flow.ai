import { connectBrowser } from '@genesy/api-client/setup'

/**
 * The Genesy API, from the browser: same-origin `/v1` (Vite's proxy in dev, Traefik in prod), the
 * session token from the shared `genesy.session` store, and a 401 signs out.
 */
export const api = connectBrowser()

// Field only: on a weak signal a request must end in "No network" with a retry, not hang.
api.defaults.timeout = 20_000
