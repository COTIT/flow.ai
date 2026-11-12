"use client"

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { ArrowDown, PanelRight, PanelRightClose, PhoneOff } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { VoiceThinking } from "@genesy/voice/components/activity"
import { useVoice } from "@genesy/voice/components/provider"
import { TranscriptRows } from "@genesy/voice/components/transcript"
import type { SessionPhase } from "@genesy/voice/lib/presentation"
import { flowStore, useFlow } from "@/lib/flow-store"
import {
  RAIL,
  band as bandOf,
  clampWidth,
  keyWidth,
  railContext,
  railOpen,
  readRail,
  sheetAfter,
  type Band,
  type RailStore,
  type SheetEvent,
  type SheetState,
} from "@/lib/rail"
import { DockSlot } from "./dock-layer"
import { OrbSlot } from "./orb-layer"

const KEY = "flow.rail"

const subscribe = (change: () => void) => {
  addEventListener("resize", change)
  return () => removeEventListener("resize", change)
}
/** The screen band (null on the server: nothing pane-specific renders before it is known). */
export const useBand = () =>
  useSyncExternalStore<Band | null>(
    subscribe,
    () => bandOf(innerWidth),
    () => null
  )
const useViewportWidth = () =>
  useSyncExternalStore(
    subscribe,
    () => innerWidth,
    () => 1280
  )
/** The results pane keeps at least this much beside the rail. */
const RESULTS_MIN = 360
/** Frame padding and the gap between the panes. */
const FRAME = 48

/** Rail memory for this tab (sessionStorage; a blocked store just forgets). */
let memory: RailStore | null = null
const listeners = new Set<() => void>()
const load = () => {
  if (memory) return memory
  let raw: string | null = null
  try {
    raw = sessionStorage.getItem(KEY)
  } catch {
    // Blocked storage: start fresh.
  }
  return (memory = readRail(raw))
}
const save = (next: RailStore) => {
  memory = next
  try {
    sessionStorage.setItem(KEY, JSON.stringify(next))
  } catch {
    // Blocked storage: remembered for this page only.
  }
  listeners.forEach((l) => l())
}
const SERVER: RailStore = { open: {}, width: null }
let seenReplies = 0
const useRailStore = () =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    load,
    () => SERVER
  )

/**
 * Where the conversation shows beside the results: nowhere (no conversation yet, or the full
 * conversation view is open), a rail (tablet/desktop) or a sheet (phone).
 */
export function useConversationLayout() {
  const voice = useVoice()
  const pathname = usePathname()
  const immersive = useFlow((s) => s.immersive)
  const resultOpen = useFlow((s) => s.resultOpen)
  const b = useBand()
  const store = useRailStore()
  const viewport = useViewportWidth()
  // Too narrow for a rail beside results that keep RESULTS_MIN (e.g. 640 px at 200% zoom): the
  // conversation is a sheet there, as on a phone (P9-7).
  const sheetOnly = b === "phone" || viewport < RAIL.min + RESULTS_MIN + FRAME
  const maxWidth = clampWidth(viewport - FRAME - RESULTS_MIN)
  const live = voice.connected || voice.starting || voice.upgrading
  const talking = live || voice.messages.length > 0
  // A start from home opens the full conversation (immersive follows a moment later).
  const fullView =
    pathname === "/" && !resultOpen && live && (immersive || voice.starting)
  const review = Boolean(voice.review?.ticket || voice.outcome)
  const context = b && b !== "phone" ? railContext(b, review) : null
  // The home dashboard has no rail or sheet: the orb continues the conversation there.
  const homeDash = pathname === "/" && !resultOpen
  const shown = Boolean(b && talking && !fullView && !homeDash)
  const open =
    shown &&
    !!b &&
    !sheetOnly &&
    railOpen({ band: b, review, stored: store.open })
  // Replies that arrived while the rail was hidden: its toggle says so (P3-2).
  const replies = voice.messages.length
  const last = voice.messages.at(-1)
  useEffect(() => {
    if (open) seenReplies = replies
  }, [open, replies])
  // A send or a reply while the rail is hidden shows it again: a hide lasts until the next turn
  // (focus stays where it was).
  // (A new line may merge into the last row, so the last row changing is the signal.)
  // A row ADDED is the signal (a reply updated in place keeps its id, so a hide holds while it
  // grows); a review card arriving counts as a reply.
  const lastId = last?.id
  const ticket = voice.review?.ticket
  const turn = useRef({ lastId, ticket })
  useEffect(() => {
    const fresh =
      (!!lastId && lastId !== turn.current.lastId) ||
      (!!ticket && ticket !== turn.current.ticket)
    turn.current = { lastId, ticket }
    if (fresh && shown && !sheetOnly && !open && context)
      save({ ...load(), open: { ...load().open, [context]: true } })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lastId, ticket])
  return {
    band: b,
    /** The full-width conversation (Conversation board) is open on home. */
    conversationView: fullView,
    /** The rail exists (open or closed) beside the results. */
    rail: shown && !sheetOnly,
    open,
    unread: !open && replies > seenReplies && !!last && last.role !== "user",
    sheet: shown && sheetOnly,
    width: Math.min(
      clampWidth(store.width ?? (b === "tablet" ? RAIL.tablet : RAIL.desktop)),
      maxWidth
    ),
    maxWidth,
    setOpen: (value: boolean) => {
      if (context)
        save({ ...load(), open: { ...load().open, [context]: value } })
    },
    setWidth: (width: number) => save({ ...load(), width: clampWidth(width) }),
  }
}

const nearBottom = (el: HTMLElement) =>
  el.scrollHeight - el.scrollTop - el.clientHeight < 48

/**
 * The transcript in a pane of its own: it scrolls by itself (one scroller per pane), follows the
 * newest line, and offers "Jump to latest" once the person has scrolled up.
 */
function PaneLog({ pane }: { pane: "rail" | "sheet" }) {
  const t = useTranslations()
  const voice = useVoice()
  const pathname = usePathname()
  const router = useRouter()
  const log = useRef<HTMLDivElement>(null)
  const [following, setFollowing] = useState(true)
  const follow = useRef(true)
  useEffect(() => {
    const el = log.current
    if (el && follow.current) el.scrollTop = el.scrollHeight
  }, [voice.messages, voice.thinking])
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={log}
        data-flow-pane={pane}
        role="log"
        aria-label={t("voice.transcript")}
        aria-live="polite"
        // Positioned: sr-only text inside stays clipped by this pane, never stretches the page.
        onScroll={(e) => {
          const at = nearBottom(e.currentTarget)
          follow.current = at
          setFollowing(at)
        }}
        className="relative flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto overscroll-y-contain px-1 py-1"
      >
        <TranscriptRows
          rows={voice.messages}
          renderResult={(row) => (
            <Button
              variant="outline"
              size="sm"
              data-role="result"
              className="min-h-11 self-start"
              onClick={() => {
                flowStore.setState({ resultOpen: true, resultFocus: row.id })
                // The Results screen is an address: without it the shell closes it again (J1).
                if (pathname !== "/") router.push("/?view=task")
              }}
            >
              <PanelRight aria-hidden />
              {t("flow.chat.showResult")}
            </Button>
          )}
        />
        <VoiceThinking />
      </div>
      {following ? null : (
        <Button
          variant="outline"
          size="sm"
          className="absolute bottom-2 left-1/2 min-h-11 -translate-x-1/2 rounded-full shadow-md"
          onClick={() => {
            const el = log.current
            if (!el) return
            follow.current = true
            setFollowing(true)
            el.scrollTop = el.scrollHeight
          }}
        >
          <ArrowDown aria-hidden />
          {t("shell.jumpToLatest")}
        </Button>
      )}
    </div>
  )
}

/** Desktop/tablet: the conversation rail on the right (orb, transcript, dock). */
export function Rail({
  phase,
  width,
  maxWidth,
  onWidth,
  onHide,
}: {
  phase: SessionPhase["phase"]
  width: number
  maxWidth: number
  onWidth: (width: number) => void
  onHide: () => void
}) {
  const t = useTranslations()
  const drag = useRef<{ x: number; width: number } | null>(null)
  return (
    <aside
      id="flow-rail"
      aria-label={t("shell.conversation")}
      style={{ width }}
      className="relative flex min-h-0 shrink-0 flex-col gap-3 rounded-3xl glass-strong p-3"
    >
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label={t("shell.resizeConversation")}
        aria-controls="flow-rail"
        aria-valuemin={RAIL.min}
        aria-valuemax={maxWidth}
        aria-valuenow={width}
        tabIndex={0}
        data-flow-resize
        onKeyDown={(e) => {
          const next = keyWidth(width, e.key)
          if (next === null) return
          e.preventDefault()
          onWidth(Math.min(next, maxWidth))
        }}
        onPointerDown={(e) => {
          drag.current = { x: e.clientX, width }
          e.currentTarget.setPointerCapture(e.pointerId)
        }}
        onPointerMove={(e) => {
          if (drag.current)
            onWidth(
              Math.min(
                maxWidth,
                drag.current.width + drag.current.x - e.clientX
              )
            )
        }}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
        className="group absolute inset-y-4 -left-4 flex w-4 cursor-col-resize touch-none items-center justify-center rounded-full focus-visible:outline-2 focus-visible:outline-(--gold)"
      >
        <span
          aria-hidden
          className="h-10 w-1 rounded-full bg-muted-foreground/30 group-hover:bg-muted-foreground/60"
        />
      </div>
      <div className="flex min-w-0 items-center gap-3">
        <OrbSlot name="compact" className="size-11" />
        <p className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
          {t(`shell.phase.${phase}`)}
        </p>
        <EndCall />
        <Button
          variant="ghost"
          size="icon"
          className="size-11 shrink-0"
          aria-label={t("shell.hideConversation")}
          aria-expanded
          aria-controls="flow-rail"
          onClick={onHide}
        >
          <PanelRightClose aria-hidden />
        </Button>
      </div>
      <PaneLog pane="rail" />
      <DockSlot />
    </aside>
  )
}

/** Phone: the conversation as a bottom sheet over the results; the dock is always in it. */
export function Sheet({ phase }: { phase: SessionPhase["phase"] }) {
  const t = useTranslations()
  const voice = useVoice()
  const [state, setState] = useState<SheetState>("peek")
  const handle = useRef<HTMLButtonElement>(null)
  const body = useRef<HTMLDivElement>(null)
  const start = useRef<number | null>(null)
  const open = state === "open"
  // While the sheet is on screen the orb lives in it (raised or lowered), so raising and
  // lowering never send it gliding across the handle under a thumb (P10-7).
  useEffect(() => {
    flowStore.setState({ sheetShown: true })
    return () => flowStore.setState({ sheetShown: false })
  }, [])
  const go = useCallback(
    (e: SheetEvent) =>
      setState((s) => {
        const next = sheetAfter(s, e)
        // Focus follows: into the conversation when raised, back to the handle when lowered.
        if (next !== s)
          requestAnimationFrame(() =>
            (next === "open" ? body.current : handle.current)?.focus()
          )
        return next
      }),
    []
  )
  // A send or a reply raises a lowered sheet (focus stays where it was).
  // A row added, not one updated in place (same id), so lowering holds while a reply grows.
  const newest = voice.messages.at(-1)?.id
  const seen = useRef(newest)
  useEffect(() => {
    if (newest && newest !== seen.current) setState("open")
    seen.current = newest
  }, [newest])
  const last = [...voice.messages]
    .reverse()
    .find((row) => row.role !== "result" && row.text)
  return (
    <section
      aria-label={t("shell.conversation")}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation()
          go({ type: "escape" })
        }
      }}
      // Raised: 62%, never less than its own handle, header and dock (with the keyboard up
      // the frame is short, and the dock must stay above the keyboard).
      style={
        open
          ? // …and never so tall that the scene above loses its room (R1/R2: 5.5rem left).
            { height: "max(62%, 15.5rem)", maxHeight: "calc(100% - 5.5rem)" }
          : undefined
      }
      className="relative flex min-h-0 shrink-0 flex-col gap-2 rounded-t-3xl glass-strong px-3 pb-3 shadow-[0_-8px_32px_rgb(28_27_24/0.14)] transition-[height] duration-240 ease-[cubic-bezier(0.32,0.72,0,1)] motion-reduce:transition-none max-sm:-mx-3 max-sm:-mb-3 [@media(max-height:500px)]:gap-1 [@media(max-height:500px)]:pb-2 max-sm:[@media(max-height:500px)]:-mx-2 max-sm:[@media(max-height:500px)]:-mb-2"
    >
      <button
        ref={handle}
        type="button"
        aria-expanded={open}
        aria-label={t(
          open ? "shell.lowerConversation" : "shell.raiseConversation"
        )}
        onClick={() => go({ type: "handle" })}
        onPointerDown={(e) => (start.current = e.clientY)}
        onPointerUp={(e) => {
          if (start.current === null) return
          const dy = e.clientY - start.current
          start.current = null
          if (Math.abs(dy) >= 40) {
            e.preventDefault()
            go({ type: "drag", dy })
          }
        }}
        // Short screens (phone landscape): a slimmer handle, so the results keep their room.
        className="flex h-11 w-32 shrink-0 touch-none justify-center self-center pt-2.5 focus-visible:outline-2 focus-visible:outline-(--gold) [@media(max-height:500px)]:absolute [@media(max-height:500px)]:top-0 [@media(max-height:500px)]:left-1/2 [@media(max-height:500px)]:z-10 [@media(max-height:500px)]:h-6 [@media(max-height:500px)]:w-20 [@media(max-height:500px)]:-translate-x-1/2 [@media(max-height:500px)]:pt-1.5"
      >
        <span
          aria-hidden
          className="h-1.5 w-11 rounded-full bg-muted-foreground/50"
        />
      </button>
      {/* Short screens: this line is just the orb, with the handle over it (the sheet keeps to
          one slim row above the dock; the last line stays for screen readers). */}
      <div className="flex min-w-0 items-center gap-2.5 [@media(max-height:500px)]:h-6">
        <OrbSlot
          name="compact"
          className="size-11 [@media(max-height:500px)]:size-6"
        />
        {open ? (
          <p className="text-sm text-muted-foreground">
            {t(`shell.phase.${phase}`)}
          </p>
        ) : (
          <p className="min-w-0 flex-1 truncate text-sm text-muted-foreground [@media(max-height:500px)]:sr-only">
            {last?.text ?? t(`shell.phase.${phase}`)}
          </p>
        )}
        {/* Short screens keep this line slim: End sits in the dock row there instead. */}
        <EndCall className="ml-auto [@media(max-height:500px)]:hidden" />
      </div>
      {open ? (
        <div
          ref={body}
          tabIndex={-1}
          className="flex min-h-0 flex-1 flex-col rounded-xl focus:outline-none focus-visible:ring-2 focus-visible:ring-(--gold)"
        >
          <PaneLog pane="sheet" />
        </div>
      ) : null}
      <DockSlot />
    </section>
  )
}

/** In the scene bar (Split board): hide or show the conversation rail. */
export function ConversationToggle({
  open,
  unread = false,
  onToggle,
}: {
  open: boolean
  /** Genesy replied while the rail was hidden. */
  unread?: boolean
  onToggle: () => void
}) {
  const t = useTranslations("shell")
  return (
    <>
      {/* The rail (and its End) is hidden: hang up from here. */}
      {open ? null : <EndCall />}
      <Button
        variant="outline"
        className="min-h-11 shrink-0 rounded-full"
        aria-expanded={open}
        aria-controls={open ? "flow-rail" : undefined}
        data-flow-show-conversation={open ? undefined : ""}
        onClick={onToggle}
      >
        {open ? <PanelRightClose aria-hidden /> : <PanelRight aria-hidden />}
        {/* A narrow results pane keeps the icon; the name stays for assistive tech. */}
        <span className="@max-[600px]:sr-only">
          {t(open ? "hideConversation" : "showConversation")}
        </span>
        {unread ? (
          <span
            data-unread
            className="flex items-center gap-1.5 text-xs font-medium"
          >
            <span aria-hidden className="size-2 rounded-full bg-(--gold)" />
            {t("newReply")}
          </span>
        ) : null}
      </Button>
    </>
  )
}

/** Hang up from where the conversation lives (rail, sheet, scene bar); only while connected. */
export function EndCall({
  className = "",
  word = false,
}: {
  className?: string
  /** Shows "End" beside the icon (home, where an icon alone is ambiguous). */
  word?: boolean
}) {
  const t = useTranslations()
  const voice = useVoice()
  if (!voice.connected) return null
  return (
    <Button
      variant="outline"
      size={word ? "default" : "icon"}
      className={`${word ? "min-h-11 px-3" : "size-11"} shrink-0 ${className}`}
      aria-label={t("voice.end")}
      onClick={() => void voice.end().catch(() => undefined)}
    >
      <PhoneOff aria-hidden />
      {word ? t("shell.endCall") : null}
    </Button>
  )
}
