// Set when Flow itself sends someone to /login (logout, expiry, Back into the app while logged
// out); a full page load never sets it, so the browser's own start is kept there.
let pending = false

/** The next login page takes focus on its heading: the control that had focus is gone. */
export const focusLoginOnArrival = () => {
  pending = true
}

/** True once per arrival announced with focusLoginOnArrival. */
export const takeLoginFocus = () => {
  const take = pending
  pending = false
  return take
}
