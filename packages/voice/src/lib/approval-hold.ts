/**
 * A host may hold approval of one review (Flow does while a design review's dry run runs or says
 * the storefront changed). Held: approving does nothing, by button or by voice; cancelling works.
 * A hold names one ticket and the login + workspace it was set under, so it never holds a new
 * review or outlives a session, workspace or user change. Office never holds.
 */
export type ApprovalHold = { ticket: string; identity: string } | null

export const approvalHeld = (
  hold: ApprovalHold,
  ticket: string | undefined,
  identity: string
) => hold !== null && hold.ticket === ticket && hold.identity === identity

export const approvalBlocked = (
  hold: ApprovalHold,
  decision: "approve" | "cancel",
  ticket: string | undefined,
  identity: string
) => decision === "approve" && approvalHeld(hold, ticket, identity)
