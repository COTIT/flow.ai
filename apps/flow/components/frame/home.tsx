"use client"

import { useEffect, useSyncExternalStore } from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"
import { ArrowRight, Mic } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { useVoice } from "@genesy/voice/components/provider"
import { DiscoveryHint } from "@/components/discovery/hint" // F3
import { flowStore, useFlow } from "@/lib/flow-store"
import { useReadiness } from "@/lib/use-readiness"
import { Noticed } from "@/components/home/noticed" // F2
import { YourFlow } from "@/components/home/your-flow" // F2
import { StudioResume } from "@/components/studio/resume" // F5
import { EndCall } from "./conversation"
import { useWorkspaceChoices, WorkspacePicker } from "./header"
import { Immersive } from "./immersive"
import { OrbCaption } from "./orb-caption"
import { OrbSlot } from "./orb-layer"
import { ApprovalScene } from "@/components/task/approval-scene" // F4

const greeting = (hour: number) =>
  hour < 12 ? "morning" : hour < 18 ? "afternoon" : "evening"
const noChange = () => () => {}
/** The local hour on the client only; the server renders no greeting, so they never disagree. */
const useLocalHour = () =>
  useSyncExternalStore(
    noChange,
    () => new Date().getHours(),
    () => null
  )

/** Set once a conversation has started on this device: the microphone note has done its job. */
const STARTED = "genesy.flow.started"
const readStarted = () => {
  try {
    return localStorage.getItem(STARTED) === "1"
  } catch {
    return false
  }
}

/**
 * Home: welcome, the orb's reserved place and its caption, then (later PRs) the discovery hint,
 * Flow noticed and Your Flow. While talking it becomes the conversation; results open as a scene.
 */
export function Home() {
  const t = useTranslations()
  const voice = useVoice()
  const me = useUsersMe()
  const workspaceId = useSession((s) => s.workspaceId)
  const { query: workspaces, choices } = useWorkspaceChoices()
  const current = choices.find((w) => w.id === workspaceId)
  const immersive = useFlow((s) => s.immersive)
  const resultOpen = useFlow((s) => s.resultOpen)
  const live = voice.connected || voice.starting || voice.upgrading
  const { state } = useReadiness()
  const hour = useLocalHour()
  // The server renders no note; the client shows it until the first conversation (P6).
  const started = useSyncExternalStore(noChange, readStarted, () => true)
  useEffect(() => {
    if (!voice.connected) return
    try {
      localStorage.setItem(STARTED, "1")
    } catch {
      // Blocked storage: the note simply keeps showing.
    }
  }, [voice.connected])
  const firstName = me.data?.data.user.name?.trim().split(/\s+/)[0]

  if (me.isError || workspaces.isError)
    return (
      <Notice
        variant="error"
        title={t("flow.error")}
        action={
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              void me.refetch()
              void workspaces.refetch()
            }}
          >
            {t("flow.retry")}
          </Button>
        }
      />
    )
  if (workspaces.isSuccess && !choices.length)
    return (
      <EmptyState
        title={t("flow.noWorkspace")}
        description={t("flow.noWorkspaceText")}
      />
    )
  if (workspaces.isSuccess && !current)
    return (
      <div className="flex flex-col gap-2">
        <h1 className="font-semibold">{t("flow.chooseWorkspace")}</h1>
        <p className="text-sm text-muted-foreground">
          {t("flow.chooseWorkspaceText")}
        </p>
        {choices.map((w) => (
          <Button
            key={w.id}
            variant="outline"
            className="justify-start"
            onClick={() => sessionStore.getState().selectWorkspace(w.id)}
          >
            {w.display_name}
          </Button>
        ))}
      </div>
    )
  if (resultOpen) return <ApprovalScene /> // F4
  if (live && immersive) return <Immersive />
  const last = [...voice.messages]
    .reverse()
    .find((row) => row.role !== "result" && row.text)
  // The orb, and the words under it, do one thing: start talking, or go back to the conversation.
  const orbLabel = t(
    voice.connected ? "shell.continueConversation" : "shell.tapToTalk"
  )
  const orbAction = () =>
    voice.connected
      ? flowStore.setState({ immersive: true, resultOpen: false })
      : void voice.start(false)
  return (
    <div className="flex flex-col items-center gap-5 py-6 text-center max-md:py-3">
      <div className="flex flex-col gap-1">
        {hour === null ? null : (
          <p className="text-sm text-muted-foreground">
            {firstName
              ? t(`shell.greetingName.${greeting(hour)}`, { name: firstName })
              : t(`shell.${greeting(hour)}`)}
          </p>
        )}
        <h1 className="text-2xl font-semibold text-balance">
          {t("shell.flowTitle")}
        </h1>
        <p className="max-w-md text-sm text-balance text-muted-foreground">
          {t("shell.flowText")}
        </p>
      </div>
      {/* P6: which business, and the shop, always in sight: one calm line, two things only. */}
      {/* Its height is kept from the first paint: the orb's place below never jumps. */}
      <div
        role="group"
        aria-label={t("shell.business")}
        className="-my-2 flex min-h-11 max-w-full flex-wrap items-center justify-center gap-x-1 text-sm"
      >
        {current ? (
          <>
            {choices.length > 1 ? (
              <WorkspacePicker
                from="home"
                label={t("shell.switchBusiness")}
                className="min-h-11 max-w-56 border-transparent bg-transparent font-medium shadow-none hover:bg-accent dark:bg-transparent"
              />
            ) : (
              <span className="px-3 font-medium">{current.display_name}</span>
            )}
            <span aria-hidden className="text-muted-foreground">
              ·
            </span>
            <Link
              href="/storefront"
              prefetch={false}
              className={buttonVariants({
                variant: "ghost",
                className: "min-h-11 font-medium",
              })}
            >
              {t("shell.yourShop")}
              <ArrowRight aria-hidden />
            </Link>
          </>
        ) : null}
      </div>
      {/* The keyboard's and screen reader's orb: its place in the page, first in the content. */}
      <button
        type="button"
        data-home-orb
        aria-label={orbLabel}
        onClick={orbAction}
        className="rounded-full focus-visible:ring-4 focus-visible:ring-(--gold) focus-visible:ring-offset-4 focus-visible:ring-offset-background focus-visible:outline-none"
      >
        <OrbSlot name="ambient" className="size-40 sm:size-56" />
      </button>
      {/* The orb is the one primary action; the words under it say what a tap does. */}
      {/* The words are a second, larger target for a pointer (the orb button above is the
          keyboard's and screen reader's one). */}
      {voice.connected ? (
        <div className="flex max-w-full items-center gap-3">
          <button
            type="button"
            tabIndex={-1}
            aria-hidden
            onClick={orbAction}
            className="min-h-11 min-w-0 rounded-xl px-2 py-1 hover:bg-accent"
          >
            <span className="block text-base font-medium">
              {t("shell.continueConversation")}
            </span>
            {last ? (
              <span className="block max-w-xs truncate text-sm text-muted-foreground sm:max-w-md">
                {last.text}
              </span>
            ) : null}
          </button>
          <EndCall />
        </div>
      ) : live ? (
        <OrbCaption />
      ) : (
        <Button
          variant="ghost"
          tabIndex={-1}
          aria-hidden
          className="min-h-11 text-base font-medium"
          onClick={orbAction}
        >
          <Mic aria-hidden />
          {t("shell.tapToTalk")}
        </Button>
      )}
      {!live && state === "available" ? (
        <>
          <Button
            variant="outline"
            className="min-h-11"
            onClick={() =>
              void voice
                .start(true)
                .then(() =>
                  requestAnimationFrame(() =>
                    document
                      .querySelector<HTMLElement>("[data-flow-composer]")
                      ?.focus()
                  )
                )
            }
          >
            {t("shell.typeInstead")}
          </Button>
          {started ? null : (
            <p className="max-w-sm text-xs text-muted-foreground">
              {t("voice.micHint")}
            </p>
          )}
        </>
      ) : null}
      <StudioResume /> {/* F5 */}
      <DiscoveryHint /> {/* F3 */}
      <Noticed /> {/* F2 */}
      <YourFlow /> {/* F2 */}
    </div>
  )
}
