"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"

// The voice provider remounts the signed-in frame per workspace (by design), and this picker with
// it: the switch made here is remembered across that remount (module scope, this tab only).
let switched: { id: string; name: string; from: string } | null = null

/**
 * P9-3: after a switch from the picker, focus is back on its trigger and the switch is said once
 * in a polite status (filled after mount, so a screen reader hears the change).
 */
export function useWorkspaceSwitch(
  workspaceId: string | null,
  /** Which picker this is: the one the switch was made from gets focus back and says it. */
  from = "header"
) {
  const t = useTranslations("shell")
  const trigger = useRef<HTMLButtonElement>(null)
  const [said, setSaid] = useState("")
  useEffect(() => {
    const done = switched
    if (!done || done.id !== workspaceId || done.from !== from) return
    switched = null
    // After the header's own mount effects, which send focus left inside it to the content.
    const focus = setTimeout(() =>
      trigger.current?.focus({ preventScroll: true })
    )
    const say = setTimeout(
      () => setSaid(t("workspaceSwitched", { name: done.name })),
      100
    )
    return () => {
      clearTimeout(focus)
      clearTimeout(say)
    }
  }, [workspaceId, t, from])
  const remember = (id: string, name: string) => {
    if (id !== workspaceId) switched = { id, name, from }
  }
  return { trigger, said, remember }
}
