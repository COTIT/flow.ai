"use client"

import { useEffect, useState } from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { RefreshCw } from "lucide-react"

import {
  getFlowScenesListQueryKey,
  useFlowScenesDelete,
  useFlowScenesList,
  useFlowScenesUpdate,
} from "@genesy/api-client/endpoints/flow-scenes"
import type { FlowPeriod, FlowScene } from "@genesy/api-client/model"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { componentFor } from "@/components/registry"
import { Freshness } from "@/components/registry/data-state"
import { BENTO_CLASS, BENTO_GRID, bentoSize } from "@/lib/bento"
import { BUILT_IN, keepKey, pendingFirst, sceneLayout } from "@/lib/scenes"
import { flowStore } from "@/lib/flow-store"
import { TTL } from "@/lib/scene-data"
import {
  useRefreshOnOutcome,
  useRefreshScenes,
  useWorkspace,
} from "@/lib/use-scene-data"

const PERIODS: FlowPeriod[] = [
  "this_week",
  "last_week",
  "last_7_days",
  "this_month",
]

export function useScenes() {
  const ws = useWorkspace()
  return useFlowScenesList(ws, {
    query: { enabled: !!ws, staleTime: Infinity },
  })
}

/** "Morning Pulse", or "Sales · This month" for a kept sales view. */
export function useSceneTitle() {
  const t = useTranslations("scenes")
  return (scene: FlowScene) => {
    const layout = sceneLayout(scene.scene)
    if ((BUILT_IN as readonly string[]).includes(scene.scene))
      return t(`scene.${layout}.title`)
    const period = scene.config.components.find((c) => c.period)?.period
    return layout === "sales" && period
      ? t("custom.sales", { period: t(`period.${period}`) })
      : t("custom.stock")
  }
}

/** One full-screen scene, drawn from its saved config through the component registry. */
export function SceneView({ sceneKey }: { sceneKey: string }) {
  const t = useTranslations("scenes")
  const ws = useWorkspace()
  const router = useRouter()
  const client = useQueryClient()
  const list = useScenes()
  const title = useSceneTitle()
  const refresh = useRefreshScenes()
  useRefreshOnOutcome()
  const [said, say] = useState("")
  const [period, setPeriod] = useState<FlowPeriod | null>(null)
  const saved = {
    onSuccess: () =>
      client.invalidateQueries({ queryKey: getFlowScenesListQueryKey(ws) }),
  }
  const update = useFlowScenesUpdate({ mutation: saved })
  const remove = useFlowScenesDelete({ mutation: saved })
  const layout = sceneLayout(sceneKey)
  const scene = list.data?.data.find((s) => s.scene === sceneKey)
  const shown = scene
    ? period
      ? scene.config.components.map((c) => (c.period ? { ...c, period } : c))
      : scene.config.components
    : []
  // The agent knows what this scene shows (ids and types only), until the scene closes.
  const visible = JSON.stringify(
    shown
      .filter((c) => !scene?.restricted.includes(c.id))
      .map(({ id, type, metric, period: p }) => ({
        id,
        type,
        metric,
        period: p,
      }))
  )
  useEffect(() => {
    flowStore.setState({ visible: JSON.parse(visible) })
    return () => flowStore.setState({ visible: null })
  }, [visible])

  if (!layout || (list.isSuccess && !scene))
    return <Notice variant="info" title={t("notFound")} />
  if (list.isError)
    return (
      <Notice
        variant="error"
        title={t("failed")}
        action={
          <Button
            variant="outline"
            size="sm"
            onClick={() => void list.refetch()}
          >
            {t("retry")}
          </Button>
        }
      />
    )
  if (!scene) return <Skeleton className="h-40 w-full rounded-2xl" />

  const builtIn = (BUILT_IN as readonly string[]).includes(sceneKey)
  const drawn = shown.filter((c) => componentFor(c.type))
  const keptKey = period ? keepKey(period) : null
  const kept =
    !!keptKey && !!list.data?.data.some((s) => s.scene === keptKey && !s.hidden)
  const write = (
    body: Parameters<typeof update.mutate>[0]["data"],
    key = sceneKey,
    done = t("saved")
  ) => {
    // A new write: no stale "Saved", no old error from the other mutation (its own resets on mutate).
    say("")
    remove.reset()
    update.mutate(
      { workspace: ws, scene: key, data: body },
      { onSuccess: () => say(done) }
    )
  }

  return (
    <article aria-labelledby="scene-title" className="flex flex-col gap-4">
      <header className="flex flex-col gap-1">
        <h1 id="scene-title" className="text-xl font-semibold">
          {title(scene)}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t(`scene.${layout}.lead`)}
        </p>
        <div className="flex flex-wrap items-center gap-2 pt-1">
          <Freshness staleAfter={TTL.attention} />
          <Button
            variant="ghost"
            size="sm"
            className="min-h-11"
            onClick={() => void refresh()}
          >
            <RefreshCw aria-hidden />
            {t("refresh")}
          </Button>
          {sceneKey === "pulse" ? (
            <Button
              variant="outline"
              size="sm"
              className="min-h-11"
              disabled={update.isPending}
              onClick={() =>
                write(
                  { config: pendingFirst(scene.config) },
                  "pulse",
                  t("pendingFirstDone")
                )
              }
            >
              {t("pendingFirst")}
            </Button>
          ) : null}
          {layout === "sales" && builtIn ? (
            <>
              <label className="flex items-center gap-2 text-sm">
                {t("periodLabel")}
                <select
                  value={period ?? ""}
                  onChange={(e) =>
                    setPeriod((e.target.value || null) as FlowPeriod | null)
                  }
                  className="min-h-11 rounded-full border bg-background px-3 text-sm"
                >
                  <option value="">{t("period.this_week")}</option>
                  {PERIODS.map((p) => (
                    <option key={p} value={p}>
                      {t(`period.${p}`)}
                    </option>
                  ))}
                </select>
              </label>
              {keptKey ? (
                <Button
                  variant="outline"
                  size="sm"
                  className="min-h-11"
                  disabled={kept || update.isPending}
                  onClick={() =>
                    write({ config: { components: shown } }, keptKey)
                  }
                >
                  {kept ? t("kept") : t("keep")}
                </Button>
              ) : null}
            </>
          ) : null}
          <Button
            variant="ghost"
            size="sm"
            className="min-h-11"
            disabled={update.isPending || remove.isPending}
            onClick={() => {
              say("")
              const home = { onSuccess: () => router.push("/") }
              if (builtIn)
                update.mutate(
                  { workspace: ws, scene: sceneKey, data: { hidden: true } },
                  home
                )
              else remove.mutate({ workspace: ws, scene: sceneKey }, home)
            }}
          >
            {t("remove")}
          </Button>
        </div>
      </header>
      <p role="status" className="text-sm text-muted-foreground empty:hidden">
        {said}
      </p>
      {update.isError || remove.isError ? (
        <Notice variant="error" title={t("saveFailed")} />
      ) : null}
      {layout === "studio" ? (
        <Link
          href="/storefront"
          className={buttonVariants({
            variant: "outline",
            className: "min-h-11 self-start",
          })}
        >
          {t("scene.studio.title")}
        </Link>
      ) : null}
      <div className="@container">
        <div data-bento className={BENTO_GRID}>
          {drawn.map((component) => {
            const Component = componentFor(component.type)
            return Component ? (
              <div
                key={component.id}
                data-component={component.id}
                data-size={bentoSize(component.type, drawn.length)}
                className={`min-w-0 [&>*]:h-full ${BENTO_CLASS[bentoSize(component.type, drawn.length)]}`}
              >
                <Component
                  component={component}
                  restricted={scene.restricted.includes(component.id)}
                />
              </div>
            ) : null
          })}
        </div>
      </div>
    </article>
  )
}
