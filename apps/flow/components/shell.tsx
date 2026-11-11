"use client"

import { useEffect, useRef } from "react"
import { usePathname } from "next/navigation"

import { useSession } from "@genesy/api-client/auth"
import { useVoice } from "@genesy/voice/components/provider"
import { sessionPhase } from "@genesy/voice/lib/presentation"
import { sceneContext } from "@genesy/voice/lib/scene-context"
import { useProposalHold } from "@/components/task/storefront-proposal"
import {
  Rail,
  Sheet,
  ConversationToggle,
  useConversationLayout,
} from "@/components/frame/conversation"
import { DockLayer, DockSlot } from "@/components/frame/dock-layer"
import { Header } from "@/components/frame/header"
import { OrbLayer } from "@/components/frame/orb-layer"
import { SceneBar } from "@/components/frame/scene-bar"
import { flowStore, useFlow } from "@/lib/flow-store"
import { useTaskWatch } from "@/lib/task-store" // F4

/** Whose task scene the address last showed (the shell may remount on a switch). */
let lastOwner: string | null = null

export { useWorkspaceChoices } from "@/components/frame/header"

/**
 * The signed-in frame, mounted once: header, viewport (content + the one orb), dock.
 * Navigation swaps only the content; the voice session and the orb stay.
 */
export function Shell({ children }: { children: React.ReactNode }) {
  const voice = useVoice()
  const pathname = usePathname()
  const endedByUser = useFlow((s) => s.endedByUser)
  const resultOpen = useFlow((s) => s.resultOpen)
  const selected = useFlow((s) => s.selected)
  const visible = useFlow((s) => s.visible)
  const { phase } = sessionPhase({ ...voice, endedByUser })
  useProposalHold(voice.review)
  useTaskWatch(voice) // F4: withdrawn proposals and receipts outlive navigation

  // Lifecycle: a start opens the conversation; losing the connection marks it ended.
  const wasConnected = useRef(false)
  useEffect(() => {
    if (voice.starting) flowStore.getState().started()
  }, [voice.starting])
  useEffect(() => {
    if (wasConnected.current && !voice.connected && !voice.upgrading)
      flowStore.getState().ended()
    wasConnected.current = voice.connected
  }, [voice.connected, voice.upgrading])

  // A new action to approve opens the result scene on home; once cancelled, the scene it opened
  // closes again; once approved it stays, as the approval turns into its receipt. // F4
  const ticket = voice.review?.ticket
  const openedForReview = useRef(false)
  useEffect(() => {
    if (ticket) {
      if (pathname !== "/" || flowStore.getState().resultOpen) return
      openedForReview.current = true
      flowStore.setState({ resultOpen: true })
    } else if (openedForReview.current) {
      openedForReview.current = false
      if (voice.outcome?.status === "cancelled")
        flowStore.setState({ resultOpen: false })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ticket])

  // Another workspace or login never sees this one's task scene.
  const owner = useSession((s) => `${s.workspaceId}:${s.token}`)
  useEffect(() => {
    if (
      lastOwner !== null &&
      lastOwner !== owner &&
      location.search.includes("view=task")
    )
      history.replaceState(history.state, "", location.pathname)
    lastOwner = owner
  }, [owner])

  // A new scene starts at its top, never at the scroll position of the page before.
  const scroller = useRef<HTMLElement>(null)
  useEffect(() => {
    scroller.current?.scrollTo(0, 0)
  }, [pathname, resultOpen])

  // The task scene is an address (`/?view=task`): Back, Forward and reload find it again.
  useEffect(() => {
    const sync = () => {
      const open =
        location.pathname === "/" &&
        new URLSearchParams(location.search).get("view") === "task"
      if (flowStore.getState().resultOpen !== open)
        flowStore.setState({ resultOpen: open })
    }
    sync()
    addEventListener("popstate", sync)
    return () => removeEventListener("popstate", sync)
  }, [pathname])
  useEffect(
    () =>
      flowStore.subscribe((state, before) => {
        if (state.resultOpen === before.resultOpen || location.pathname !== "/")
          return
        const url = new URL(location.href)
        if ((url.searchParams.get("view") === "task") === state.resultOpen)
          return
        if (state.resultOpen) url.searchParams.set("view", "task")
        else url.searchParams.delete("view")
        history.pushState(null, "", url.pathname + url.search)
      }),
    []
  )

  // The agent quietly knows the scene (ids only); the provider sends only while connected.
  const send = voice.sendContext
  const onResults = pathname === "/" && resultOpen
  const results = onResults
    ? voice.messages.filter((row) => row.role === "result" && row.views?.[0])
    : []
  const context = sceneContext({
    pathname: onResults ? "/results" : pathname,
    selected,
    visible,
    results,
    reviewOpen: !!voice.review,
  })
  // Sent again only when what it says changes (a redraw in place keeps ids but not the drawing).
  const contextKey = JSON.stringify(context)
  useEffect(() => {
    send(context)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [contextKey])

  // The dock stays above a phone's keyboard: the frame follows the visual viewport.
  useEffect(() => {
    const view = window.visualViewport
    if (!view) return
    // Never taller than the window: mid-rotation the visual viewport can lag behind it (INT-2).
    const fit = () =>
      document.documentElement.style.setProperty(
        "--flow-vh",
        `${Math.min(view.height, window.innerHeight)}px`
      )
    fit()
    view.addEventListener("resize", fit)
    window.addEventListener("resize", fit)
    return () => {
      view.removeEventListener("resize", fit)
      window.removeEventListener("resize", fit)
    }
  }, [])

  // Leaving the signed-in app (logout, another login) forgets the visual state.
  useEffect(
    () => () => flowStore.setState(flowStore.getInitialState(), true),
    []
  )

  const home = pathname === "/"
  const pane = useConversationLayout()
  const railShown = pane.rail && pane.open
  // A conversation started away from home shows where it started (P3-2): a closed rail opens.
  const startingHere = voice.starting && pane.rail && !pane.open
  useEffect(() => {
    if (startingHere) pane.setOpen(true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [startingHere])
  const bottomDock = !railShown && !pane.sheet
  return (
    <div
      style={
        {
          height: "var(--flow-vh, 100dvh)",
          "--flow-safe-top": "env(safe-area-inset-top, 0px)",
          // The dock's height until it is measured (its slot reserves it from the first paint).
          "--flow-dock-h": "68px",
        } as React.CSSProperties
      }
      // A double-click acts once: the layout may change under the pointer after the first click,
      // so the second click of a double-click never reaches a control (P2-2).
      onClickCapture={(e) => {
        const control =
          e.target instanceof Element
            ? e.target.closest("button, a, [role=menuitem]")
            : null
        // A toggle (handle, show/hide, select) flips back on the next press by design (P10-5).
        if (
          e.detail > 1 &&
          control &&
          !control.matches("[aria-expanded], [aria-pressed]")
        ) {
          e.preventDefault()
          e.stopPropagation()
        }
      }}
      className={`relative grid grid-cols-[minmax(0,1fr)] gap-3 p-3 pt-[calc(var(--flow-safe-top)+0.75rem)] md:gap-4 md:p-4 [@media(max-height:500px)]:gap-2 [@media(max-height:500px)]:p-2 [@media(max-height:500px)]:pt-[calc(var(--flow-safe-top)+0.5rem)] [@media(pointer:coarse)_and_(min-height:501px)]:pt-[calc(var(--flow-safe-top)+2.25rem)] ${bottomDock ? "grid-rows-[minmax(0,1fr)_auto]" : "grid-rows-[minmax(0,1fr)]"}`}
    >
      {/* Floats over the frame; never takes a row, so it never pushes content. */}
      <Header phase={phase} />
      {/* Split view: results and the conversation are panes side by side (a sheet on a phone);
          each pane scrolls on its own, the page never does. */}
      <div className="relative flex min-h-0 min-w-0 gap-4 max-sm:flex-col max-sm:gap-0">
        <div className="@container relative flex min-h-0 min-w-0 flex-1 flex-col">
          {!home || resultOpen ? (
            <SceneBar
              orb={!railShown && !pane.sheet}
              conversation={
                pane.rail ? (
                  <ConversationToggle
                    open={pane.open}
                    unread={pane.unread}
                    onToggle={() => pane.setOpen(!pane.open)}
                  />
                ) : null
              }
            />
          ) : null}
          {/* The results pane's one scroller; positioned so nothing inside (e.g. sr-only text)
              stretches the page. */}
          <main
            ref={scroller}
            data-flow-scroll
            data-flow-pane="results"
            data-flow-content
            id="flow-content"
            tabIndex={-1}
            className="relative min-h-0 flex-1 overflow-y-auto overscroll-y-contain focus:outline-none"
          >
            <div
              className={`mx-auto flex w-full min-w-0 flex-col gap-5 max-md:gap-3 ${home && !resultOpen && !pane.conversationView ? "max-w-3xl" : ""}`}
            >
              {children}
            </div>
          </main>
        </div>
        {railShown ? (
          <Rail
            phase={phase}
            width={pane.width}
            maxWidth={pane.maxWidth}
            onWidth={pane.setWidth}
            onHide={() => pane.setOpen(false)}
          />
        ) : null}
        {pane.sheet ? <Sheet phase={phase} /> : null}
        <OrbLayer />
      </div>
      {bottomDock ? <DockSlot /> : null}
      {/* The one dock, placed over the slot the layout shows (bottom, rail or sheet). */}
      <DockLayer phase={phase} />
    </div>
  )
}
