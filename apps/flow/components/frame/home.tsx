"use client"

import { useSyncExternalStore } from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { useVoice } from "@genesy/voice/components/provider"
import { DiscoveryHint } from "@/components/discovery/hint" // F3
import { useFlow } from "@/lib/flow-store"
import { useReadiness } from "@/lib/use-readiness"
import { Noticed } from "@/components/home/noticed" // F2
import { YourFlow } from "@/components/home/your-flow" // F2
import { StudioResume } from "@/components/studio/resume" // F5
import { useWorkspaceChoices } from "./header"
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
  const sheetShown = useFlow((s) => s.sheetShown)
  const live = voice.connected || voice.starting || voice.upgrading
  const { state } = useReadiness()
  const hour = useLocalHour()

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
  return (
    <div className="flex flex-col items-center gap-5 py-6 text-center max-md:py-3">
      <div className="flex flex-col gap-1">
        {hour === null ? null : (
          <p className="text-sm text-muted-foreground">
            {t(`shell.${greeting(hour)}`)}
          </p>
        )}
        <h1 className="text-2xl font-semibold text-balance">
          {t("shell.flowTitle")}
        </h1>
        <p className="max-w-md text-sm text-balance text-muted-foreground">
          {t("shell.flowText")}
        </p>
      </div>
      {/* With the phone sheet on screen the orb lives there: no empty place for it here. */}
      <OrbSlot
        name="ambient"
        className={sheetShown ? "hidden" : "size-40 sm:size-56"}
      />
      <OrbCaption />
      {!live && state === "available" ? (
        <>
          <Button
            variant="outline"
            className="min-h-11"
            onClick={() => void voice.start(true)}
          >
            {t("voice.textInstead")}
          </Button>
          <p className="max-w-sm text-xs text-muted-foreground">
            {t("voice.micHint")}
          </p>
        </>
      ) : null}
      <StudioResume /> {/* F5 */}
      <DiscoveryHint /> {/* F3 */}
      <Noticed /> {/* F2 */}
      <YourFlow /> {/* F2 */}
    </div>
  )
}
