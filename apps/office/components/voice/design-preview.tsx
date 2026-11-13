"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { VoiceConversation } from "./conversation"
import { VoiceContext, type VoiceContextValue } from "@genesy/voice/components/provider"
import type { VoiceReview } from "@genesy/voice/lib/client-contract"

/** Fake conversation states so the real voice screen can be reviewed without a provider session. */
const STATES = [
  "idle",
  "listening",
  "thinking",
  "speaking",
  "review",
  "review-batch",
  "completed",
  "failed",
  "text",
] as const
type State = (typeof STATES)[number]

const review: VoiceReview = {
  operationId: "stock_set_levels",
  method: "POST",
  ticket: "t",
  summary: "Set stock levels for Honey 500 g",
  confirmationCode: "417 802",
  confirmationPhrase: "Approve stock update 417 802",
  expiresAt: new Date(Date.now() + 120_000).toISOString(),
  details: {
    product: "Honey 500 g (HON-500)",
    place: "Main warehouse",
    quantity: "120 jars (was 84)",
    reason: "Delivery from Ruaha cooperative",
    note: "Lot RH-2026-11 added to batch 7",
  },
}
const batch: VoiceReview = {
  ...review,
  kind: "batch",
  summary: "Prepare order 1042 for pickup",
  confirmationPhrase: "Approve order plan 417 802",
  steps: [
    {
      actionId: "a1",
      operationId: "orders_pick",
      method: "POST",
      summary: "Pick order 1042",
      details: { order: "#1042", lines: "3 lines, 14 items" },
    },
    {
      actionId: "a2",
      operationId: "invoices_create",
      method: "POST",
      summary: "Create invoice",
      details: { customer: "Mama Lishe Café", total: "TSh 86,500" },
    },
    {
      actionId: "a3",
      operationId: "invoices_send",
      method: "POST",
      summary: "Send invoice",
      details: { to: "orders@mamalishe.example", channel: "Email" },
    },
  ],
}
const transcript = [
  { role: "agent", text: "Good morning. What shall we start with?" },
  { role: "user", text: "Set the honey stock to 120 jars, we got the Ruaha delivery." },
  { role: "agent", text: "Checking the honey stock first…" },
]

export function VoiceDesignPreview() {
  const [state, setState] = useState<State>("idle")
  const [tick, setTick] = useState(0)
  const start = useRef(0)
  useEffect(() => {
    start.current = Date.now()
    const timer = setInterval(() => setTick((n) => n + 1), 1200)
    return () => clearInterval(timer)
  }, [state])
  const phase = Math.min(tick, 3)
  const noop = async () => {}
  const value = useMemo<VoiceContextValue>(() => {
    const connected = state !== "idle"
    const level = () => {
      const t = (Date.now() - start.current) / 1000
      return 0.35 + 0.3 * Math.sin(t * 6) + 0.2 * Math.sin(t * 17)
    }
    return {
      historyEnabled: false,
      logicalConversationId: null,
      historyRestoring: false,
      restoreConversation: noop,
      continueConversation: noop,
      startNewConversation: noop,
      endConversation: noop,
      deleteConversation: noop,
      connected,
      starting: false,
      upgrading: false,
      busy: false,
      thinking: state === "thinking",
      requesting: false,
      textOnly: state === "text",
      muted: false,
      capturePaused: false,
      speaking: state === "speaking",
      messages:
        state === "idle"
          ? []
          : transcript
              .slice(0, state === "listening" ? 1 : state === "text" ? 2 : 3)
              .map((row, i) => ({ id: String(i), ...row }) as never),
      attachments: [],
      review:
        state === "review" ? review : state === "review-batch" ? batch : null,
      decided: null,
      outcome:
        state === "completed"
          ? { status: "completed", message: "Stock levels updated." }
          : state === "failed"
            ? { status: "failed", message: "The request could not be completed." }
            : null,
      batchProgress: null,
      error: state === "failed" ? "The request could not be completed." : "",
      recovery: null,
      recoveredTasks: [],
      checkingRecovery: false,
      recoveredReview: false,
      approvalHeld: false,
      holdApproval: () => undefined,
      progress: null,
      narration: "",
      steps:
        state === "thinking"
          ? ["Finding the product", "Checking current stock", "Preparing the review"].slice(
              0,
              phase + 1
            )
          : [],
      inputLevel: () => (state === "listening" ? level() : 0),
      outputLevel: () => (state === "speaking" ? level() : 0),
      refreshRecovery: noop,
      restore: noop,
      stopBatch: noop,
      refreshBatch: noop,
      closeRecovery: noop,
      start: async (textOnly) => setState(textOnly ? "text" : "listening"),
      upgrade: noop,
      end: async () => setState("idle"),
      toggleMute: () => {},
      micMode: "open" as const,
      setMicMode: () => {},
      talk: () => {},
      pauseCapture: () => {},
      stopSpeaking: () => setState("listening"),
      send: async () => {
        setState("thinking")
        return true
      },
      activity: () => {},
      choose: () => {},
      remove: noop,
      retryAttachment: async () => true,
      confirm: async (decision) =>
        setState(decision === "approve" ? "completed" : "listening"),
      unfinished: null,
      resume: noop,
      resolveHref: (href) => href,
      sendContext: noop,
    }
  }, [state, phase])
  return (
    <VoiceContext.Provider value={value}>
      <div
        data-design-switcher=""
        className="fixed top-2 left-1/2 z-[60] flex max-w-[calc(100%-1rem)] -translate-x-1/2 flex-wrap justify-center gap-1 rounded-full bg-black/70 p-1 text-xs text-white backdrop-blur"
      >
        {STATES.map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => setState(s)}
            aria-pressed={state === s}
            className="rounded-full px-2.5 py-1 aria-pressed:bg-(--gold) aria-pressed:text-black"
          >
            {s}
          </button>
        ))}
      </div>
      <VoiceConversation />
    </VoiceContext.Provider>
  )
}
