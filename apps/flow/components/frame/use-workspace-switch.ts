"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"

// The voice provider remounts the signed-in frame per workspace (by design), and this picker with
// it: the switch made here is remembered across that remount (module scope, this tab only).
let switched: { id: string; name: string } | null = null

/**
 * P9-3: after a switch from the picker, focus is back on its trigger and the switch is said once
 * in a polite status (filled after mount, so a screen reader hears the change).
 */
export function useWorkspaceSwitch(workspaceId: string | null) {
  const t = useTranslations("shell")
  const trigger = useRef<HTMLButtonElement>(null)
  const [said, setSaid] = useState("")
  useEffect(() => {
    const done = switched
    if (!done || done.id !== workspaceId) return
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
  }, [workspaceId, t])
  const remember = (id: string, name: string) => {
    if (id !== workspaceId) switched = { id, name }
  }
  return { trigger, said, remember }
}
