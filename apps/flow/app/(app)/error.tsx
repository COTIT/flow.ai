"use client"

import { ScreenError } from "@/components/screen-error"

/** Inside the signed-in frame: a failed screen keeps the header, the dock, the conversation and the draft. */
export default function AppError(props: { error: Error; reset: () => void }) {
  return <ScreenError {...props} />
}
