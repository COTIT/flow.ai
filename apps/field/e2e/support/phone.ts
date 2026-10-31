/**
 * A fresh Tanzanian mobile number for one registration.
 *
 * The phone is the farmer's login name and registration now creates that
 * login straight away, so two registrations with one number would collide on
 * `this phone number already has an app login`. Cleanup removes the login
 * with the E2E- person it belongs to.
 */
export function markedPhone(): string {
  return `+2557${String(Math.floor(Math.random() * 1e8)).padStart(8, '0')}`
}
