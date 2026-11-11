import { choosePresentation, type ResultView } from "@genesy/voice/lib/views"
import { VoiceError } from "./server"

const PICTURE_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export type ViewChoice = { presentation?: unknown; replaces?: string }

/** How the agent asked to show a read: never accepted for writes or secure forms. */
export function viewChoice(
  input: { presentation?: unknown; replaces?: unknown },
  operation: { write: boolean; disposition: string }
): ViewChoice | undefined {
  const { presentation, replaces } = input
  if (presentation === undefined && replaces === undefined) return undefined
  if (operation.write || operation.disposition !== "execute")
    throw new VoiceError("A view choice applies only to reads.")
  return {
    presentation,
    replaces: typeof replaces === "string" && PICTURE_ID.test(replaces) ? replaces.toLowerCase() : undefined,
  }
}

/** The read's picture as asked, if it supports that; otherwise its default and why. Data untouched. */
export function present<T extends object>(outcome: T & { view?: unknown }, choice: ViewChoice | undefined): T {
  if (!choice || !outcome.view) return outcome
  const { view, note } =
    choice.presentation === undefined
      ? { view: outcome.view as ResultView, note: undefined }
      : choosePresentation(outcome.view as ResultView, choice.presentation)
  return {
    ...outcome,
    view,
    ...(note ? { presentation_note: note } : {}),
    ...(choice.replaces ? { replaces: choice.replaces } : {}),
  }
}
