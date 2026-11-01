/**
 * The temporary password a farmer just signed in with, so the forced change can send it as
 * `current_password` without asking for it twice. Memory only: never localStorage, sessionStorage
 * or the URL. The set-password screen copies it into its own state and clears it at once; a
 * reload simply asks the farmer to type it.
 */
let held: string | null = null

export function holdTempPassword(password: string) {
  held = password
}

export function peekTempPassword(): string | null {
  return held
}

export function clearTempPassword() {
  held = null
}
