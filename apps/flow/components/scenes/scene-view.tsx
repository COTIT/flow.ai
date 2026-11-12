"use client"

import { useEffect, useRef, useState } from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { MoreHorizontal, RefreshCw } from "lucide-react"

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
import { ComparePanel } from "@/components/registry/comparison"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@genesy/ui/components/dropdown-menu"
import { Freshness } from "@/components/registry/data-state"
import {
  BENTO_GRID,
  BENTO_PLACE,
  BENTO_SINGLE,
  bentoSize,
  placements,
} from "@/lib/bento"
import { BUILT_IN, keepKey, pendingFirst, sceneLayout } from "@/lib/scenes"
import { flowStore } from "@/lib/flow-store"
import { TTL } from "@/lib/scene-data"
import {
  useRefreshOnOutcome,
  useRefreshScenes,
  useWorkspace,
} from "@/lib/use-scene-data"
import { BentoTile } from "./bento-tile"

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
/**
 * Beside two stacked metrics the big tile takes their height, never sets it, except a chart, whose
 * plot stays readable (≥ 160 px: the tile is ≥ 280 px); the metrics then share that height with
 * their value centred. Charts stop at 340 px.
 */
function placeClass(place: React.CSSProperties | undefined, size: string) {
  const vars = (place ?? {}) as Record<string, unknown>
  const spansRows = Number(vars["--r12"]) > 1
  return [
    spansRows ? "@min-[1024px]:[contain:size]" : "",
    spansRows && size === "chart" ? "@min-[1024px]:min-h-[280px]" : "",
    vars["--c12"] === "1"
      ? "@min-[1024px]:[&>*:first-child]:justify-center"
      : "",
    size === "chart" ? "@min-[640px]:max-h-[340px]" : "",
  ].join(" ")
}

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
  const [comparing, setComparing] = useState(false)
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
  const writing = useRef(false)
  // A write is done once the saved scenes are read again (its success refetches them).
  useEffect(() => {
    writing.current = false
  }, [list.dataUpdatedAt])
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
  // Already in that order: nothing to do until the order changes.
  const ordered =
    JSON.stringify(pendingFirst(scene.config)) === JSON.stringify(scene.config)
  const drawn = shown.filter((c) => componentFor(c.type))
  // Bento fit: every row filled to the width, in the saved order.
  const placed = placements(drawn)
  const keptKey = period ? keepKey(period) : null
  // Compare: only where this scene's comparison is readable for this person.
  const compareFrom = shown.find(
    (c) => c.type === "comparison" && !scene.restricted.includes(c.id)
  )?.period

  const kept =
    !!keptKey && !!list.data?.data.some((s) => s.scene === keptKey && !s.hidden)
  const write = (
    body: Parameters<typeof update.mutate>[0]["data"],
    key = sceneKey,
    done = t("saved")
  ) => {
    // One write per press, however fast the next press comes (P2-3b): held until the saved
    // scenes are read again, or the write fails.
    if (writing.current) return
    writing.current = true
    // A new write: no stale "Saved", no old error from the other mutation (its own resets on mutate).
    say("")
    remove.reset()
    update.mutate(
      { workspace: ws, scene: key, data: body },
      {
        onSuccess: () => say(done),
        onError: () => (writing.current = false),
      }
    )
  }

  return (
    <article aria-labelledby="scene-title" className="flex flex-col gap-4">
      {/* Main board: the title on the left, its freshness and actions on the right. */}
      <header className="flex flex-wrap items-end gap-x-4 gap-y-1">
        <div className="mr-auto min-w-0">
          <h1 id="scene-title" className="text-xl font-semibold">
            {title(scene)}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t(`scene.${layout}.lead`)}
          </p>
        </div>
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
          {/* Keep Flow simple: one "More" menu holds every other action of the scene. */}
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <Button
                  variant="outline"
                  size="sm"
                  className="min-h-11 rounded-full"
                >
                  <MoreHorizontal aria-hidden />
                  {t("moreActions")}
                </Button>
              }
            />
            <DropdownMenuContent align="end" className="min-w-56">
              {sceneKey === "pulse" ? (
                <DropdownMenuItem
                  className="min-h-11"
                  disabled={update.isPending || ordered}
                  onClick={() =>
                    write(
                      { config: pendingFirst(scene.config) },
                      "pulse",
                      t("pendingFirstDone")
                    )
                  }
                >
                  {t("pendingFirst")}
                </DropdownMenuItem>
              ) : null}
              {layout === "sales" && builtIn ? (
                <>
                  <DropdownMenuGroup>
                    <DropdownMenuLabel>{t("periodLabel")}</DropdownMenuLabel>
                    <DropdownMenuRadioGroup
                      value={period ?? ""}
                      onValueChange={(value) =>
                        setPeriod((String(value) || null) as FlowPeriod | null)
                      }
                    >
                      <DropdownMenuRadioItem value="" className="min-h-11">
                        {t("period.this_week")}
                      </DropdownMenuRadioItem>
                      {PERIODS.map((p) => (
                        <DropdownMenuRadioItem
                          key={p}
                          value={p}
                          className="min-h-11"
                        >
                          {t(`period.${p}`)}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  </DropdownMenuGroup>
                  {compareFrom ? (
                    <DropdownMenuCheckboxItem
                      className="min-h-11"
                      checked={comparing}
                      onCheckedChange={(on) => setComparing(!!on)}
                    >
                      {t(
                        compareFrom === "this_week"
                          ? "compare.button"
                          : "compare.buttonBefore"
                      )}
                    </DropdownMenuCheckboxItem>
                  ) : null}
                  {keptKey ? (
                    <DropdownMenuItem
                      className="min-h-11"
                      disabled={kept || update.isPending}
                      onClick={() =>
                        write({ config: { components: shown } }, keptKey)
                      }
                    >
                      {kept ? t("kept") : t("keep")}
                    </DropdownMenuItem>
                  ) : null}
                  <DropdownMenuSeparator />
                </>
              ) : null}
              <DropdownMenuItem
                className="min-h-11"
                disabled={update.isPending || remove.isPending}
                onClick={() => {
                  if (writing.current) return
                  writing.current = true
                  say("")
                  const home = {
                    onSuccess: () => router.push("/"),
                    onError: () => (writing.current = false),
                  }
                  if (builtIn)
                    update.mutate(
                      {
                        workspace: ws,
                        scene: sceneKey,
                        data: { hidden: true },
                      },
                      home
                    )
                  else remove.mutate({ workspace: ws, scene: sceneKey }, home)
                }}
              >
                {t("remove")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
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
      {comparing && compareFrom ? <ComparePanel period={compareFrom} /> : null}
      <div className="@container">
        <div
          data-bento
          className={drawn.length > 1 ? BENTO_GRID : BENTO_SINGLE}
        >
          {drawn.map((component) => {
            const Component = componentFor(component.type)
            const size = bentoSize(component.type, drawn.length)
            return Component ? (
              <BentoTile
                key={component.id}
                id={component.id}
                size={size}
                className={
                  drawn.length > 1
                    ? // Beside two stacked metrics the big tile takes their height, never sets it.
                      `${BENTO_PLACE} ${placeClass(placed.get(component.id), size)}`
                    : ""
                }
                style={drawn.length > 1 ? placed.get(component.id) : undefined}
              >
                <Component
                  component={component}
                  restricted={scene.restricted.includes(component.id)}
                />
              </BentoTile>
            ) : null
          })}
        </div>
      </div>
    </article>
  )
}
