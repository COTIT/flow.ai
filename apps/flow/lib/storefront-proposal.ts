import type {
  StorefrontDesignCollection,
  StorefrontDesignDocument,
} from "@genesy/api-client/model"

/** A proposed design the review page hands its preview frame: the API's dry run, never saved. */
export type Proposal = {
  document: StorefrontDesignDocument
  collections: StorefrontDesignCollection[]
}

// The only two messages either side knows. Both go to this origin only, never "*".
export const PROPOSAL_READY = "genesy:storefront-proposal-ready"
export const PROPOSAL = "genesy:storefront-proposal"

const typed = (data: unknown, type: string) =>
  !!data &&
  typeof data === "object" &&
  (data as { type?: unknown }).type === type

/** The review page: only its own frame, on this origin, saying exactly "ready". */
export const readyFromFrame = (
  event: MessageEvent,
  frame: Window | null | undefined,
  origin: string
) =>
  !!frame &&
  event.source === frame &&
  event.origin === origin &&
  typed(event.data, PROPOSAL_READY)

/**
 * The frame: a proposal only from the page around it, on this origin. null = not for us (ignore);
 * "invalid" = for us, but the shared renderer cannot draw it.
 */
export function proposalFromParent(
  event: MessageEvent,
  parent: Window,
  origin: string,
  canRender: (document: never, collections: never) => boolean
): Proposal | "invalid" | null {
  if (
    event.source !== parent ||
    event.origin !== origin ||
    !typed(event.data, PROPOSAL)
  )
    return null
  const { document, collections } = event.data as Partial<Proposal>
  return document &&
    Array.isArray(collections) &&
    canRender(document as never, collections as never)
    ? { document, collections }
    : "invalid"
}

/**
 * Whether an edit review's approval is held: while its dry run runs (an approve could otherwise
 * go out before a 409 is known) and when the draft moved (409). Any other failure releases it:
 * the diff is still exact, so the person may approve (with Try again for the preview).
 */
export const holdsApproval = (
  edit: boolean,
  status: "pending" | "error" | "success",
  httpStatus?: number
) =>
  edit && (status === "pending" || (status === "error" && httpStatus === 409))
