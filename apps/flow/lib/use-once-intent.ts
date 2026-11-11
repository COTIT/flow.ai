"use client"

import { useEffect, useRef, useState } from "react"

import { useVoice } from "@genesy/voice/components/provider"

import { INTENT_LIMIT_MS, intentPending, type SentIntent } from "./once-intent"

/**
 * One message per intent for a task-card button: `send` goes out once, then the button stays
 * pending (disable it, mark it aria-busy) until Genesy answers, fails, or the card's subject
 * (`key`) changes. A failed send frees it at once.
 */
export function useOnceIntent(key: string) {
  const voice = useVoice()
  const [sent, setSent] = useState<SentIntent | null>(null)
  // Also a ref: a second click can land before React re-renders the disabled button.
  const latest = useRef<SentIntent | null>(null)
  const now = { key, rows: voice.messages, error: voice.error, connected: voice.connected }
  const pending = intentPending(sent, now)
  useEffect(() => {
    if (!pending || !sent) return
    const timer = setTimeout(() => setSent(null), sent.at + INTENT_LIMIT_MS - Date.now())
    return () => clearTimeout(timer)
  }, [pending, sent])

  async function send(text: string) {
    if (intentPending(latest.current, now)) return
    const intent = { key, after: voice.messages.at(-1)?.id, error: voice.error, at: Date.now() }
    latest.current = intent
    setSent(intent)
    if (!(await voice.send(text)) && latest.current === intent) {
      latest.current = null
      setSent(null)
    }
  }
  return { pending, send }
}
