/**
 * A host may hold approval of one review, by its ticket (Flow does while the storefront changed
 * under a design review). Held: approving does nothing, by button or by voice; cancelling works.
 * A hold names one ticket, so a new review is never held by an old hold. Office never holds.
 */
export const approvalHeld = (held: string | null, ticket: string | undefined) =>
  held !== null && held === ticket

export const approvalBlocked = (
  held: string | null,
  decision: "approve" | "cancel",
  ticket: string | undefined
) => decision === "approve" && approvalHeld(held, ticket)
