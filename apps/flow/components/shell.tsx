"use client"

import { useEffect, useRef } from "react"
import { usePathname } from "next/navigation"

import { useVoice } from "@genesy/voice/components/provider"
import { sessionPhase } from "@genesy/voice/lib/presentation"
import { sceneContext } from "@genesy/voice/lib/scene-context"
import { useProposalHold } from "@/components/task/storefront-proposal"
import { Dock } from "@/components/frame/dock"
import { Header } from "@/components/frame/header"
import { OrbLayer } from "@/components/frame/orb-layer"
import { SceneBar } from "@/components/frame/scene-bar"
import { flowStore, useFlow } from "@/lib/flow-store"
import { useTaskWatch } from "@/lib/task-store" // F4

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

  // A new scene starts at its top, never at the scroll position of the page before.
  const scroller = useRef<HTMLElement>(null)
  useEffect(() => {
    scroller.current?.scrollTo(0, 0)
  }, [pathname, resultOpen])

  // The agent quietly knows the scene (ids only); the provider sends only while connected.
  const send = voice.sendContext
  useEffect(() => {
    send(
      sceneContext({
        pathname: pathname === "/" && resultOpen ? "/results" : pathname,
        selected,
      })
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pathname, resultOpen, selected])

  // The dock stays above a phone's keyboard: the frame follows the visual viewport.
  useEffect(() => {
    const view = window.visualViewport
    if (!view) return
    const fit = () =>
      document.documentElement.style.setProperty(
        "--flow-vh",
        `${view.height}px`
      )
    fit()
    view.addEventListener("resize", fit)
    return () => view.removeEventListener("resize", fit)
  }, [])

  // Leaving the signed-in app (logout, another login) forgets the visual state.
  useEffect(
    () => () => flowStore.setState(flowStore.getInitialState(), true),
    []
  )

  const home = pathname === "/"
  return (
    <div
      style={{ height: "var(--flow-vh, 100dvh)" }}
      className="grid grid-cols-[minmax(0,1fr)] grid-rows-[auto_minmax(0,1fr)_auto] gap-3 p-3 md:gap-4 md:p-4"
    >
      <Header phase={phase} />
      <div className="relative flex min-h-0 flex-col">
        {!home || resultOpen ? <SceneBar /> : null}
        {/* The one scroll root; positioned so nothing inside (e.g. sr-only text) stretches the page. */}
        <main
          ref={scroller}
          data-flow-scroll
          className="relative min-h-0 flex-1 overflow-y-auto"
        >
          <div
            className={`mx-auto flex w-full min-w-0 flex-col gap-5 max-md:gap-3 ${home ? "max-w-3xl" : "max-w-2xl"}`}
          >
            {children}
          </div>
        </main>
        <OrbLayer />
      </div>
      <div className="mx-auto w-full max-w-3xl min-w-0">
        <Dock phase={phase} />
      </div>
    </div>
  )
}
