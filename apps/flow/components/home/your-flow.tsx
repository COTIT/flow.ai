"use client"

import { useState } from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import {
  getFlowScenesListQueryKey,
  useFlowScenesReset,
  useFlowScenesUpdate,
} from "@genesy/api-client/endpoints/flow-scenes"
import type { FlowPeriod, FlowScene } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { formatMoney } from "@genesy/ui/lib/money"
import { useSceneTitle, useScenes } from "@/components/scenes/scene-view"
import { hiddenScenes, homeScenes, sceneLayout } from "@/lib/scenes"
import {
  useAttention,
  useReports,
  useSales,
  useWorkspace,
} from "@/lib/use-scene-data"

/** One current signal per scene, from the same (deduped) queries the scene itself uses. */
function SalesSignal({ period }: { period: FlowPeriod }) {
  const locale = useLocale()
  const row = useSales(period).data?.data.currencies[0]
  return row ? (
    <>{formatMoney(row.gross_total, row.decimals, row.currency, locale)}</>
  ) : null
}
function CountSignal({
  pick,
  label,
}: {
  pick: "stock" | "orders"
  label: string
}) {
  const t = useTranslations("scenes")
  const a = useAttention().data?.data
  const n = pick === "stock" ? a?.stock?.below_minimum.count : a?.orders?.open
  return n === undefined ? null : <>{`${n} · ${t(`metric.${label.replace(".", "_")}`)}`}</>
}
function ReportSignal() {
  const t = useTranslations("scenes")
  const n = useReports().data?.data.length
  return n === undefined ? null : <>{`${n} · ${t("reports")}`}</>
}

function Signal({ scene }: { scene: FlowScene }) {
  const open = (id: string) =>
    scene.config.components.some(
      (c) => c.id === id && !scene.restricted.includes(c.id)
    )
  const can = (metric: string) =>
    scene.config.components.some(
      (c) => c.metric?.startsWith(metric) && !scene.restricted.includes(c.id)
    )
  switch (sceneLayout(scene.scene)) {
    case "pulse":
      return (
        <>
          {open("sales-today") ? <SalesSignal period="today" /> : null}
          {open("orders-pending") ? (
            <span className="block">
              <CountSignal pick="orders" label="orders.pending" />
            </span>
          ) : null}
        </>
      )
    case "stock":
      return can("stock.") ? (
        <CountSignal pick="stock" label="stock.below_minimum" />
      ) : null
    case "orders":
      return can("orders.") ? (
        <CountSignal pick="orders" label="orders.pending" />
      ) : null
    case "sales": {
      const card = scene.config.components.find(
        (c) => c.metric === "sales.gross" && !scene.restricted.includes(c.id)
      )
      return card?.period ? <SalesSignal period={card.period} /> : null
    }
    case "routines":
      return <ReportSignal />
    default:
      return null
  }
}

/** Your Flow: the scenes the person keeps, as previews; opening one reveals it in full. */
export function YourFlow() {
  const t = useTranslations("scenes")
  const ws = useWorkspace()
  const client = useQueryClient()
  const list = useScenes()
  const title = useSceneTitle()
  const [said, say] = useState("")
  const saved = {
    onSuccess: () =>
      client.invalidateQueries({ queryKey: getFlowScenesListQueryKey(ws) }),
  }
  const update = useFlowScenesUpdate({ mutation: saved })
  const reset = useFlowScenesReset({ mutation: saved })
  if (!list.data) return null
  const scenes = homeScenes(list.data.data)
  const hidden = hiddenScenes(list.data.data)
  return (
    <section
      aria-labelledby="your-flow-title"
      className="flex w-full flex-col gap-3 text-left"
    >
      <div className="flex flex-col">
        <h2
          id="your-flow-title"
          className="text-xs font-semibold tracking-wide text-muted-foreground uppercase"
        >
          {t("yourFlow")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("yourFlowText")}</p>
      </div>
      <ul className="grid gap-3 max-sm:-mx-1 max-sm:flex max-sm:snap-x max-sm:overflow-x-auto max-sm:px-1 max-sm:pb-1 sm:grid-cols-2">
        {scenes.map((scene) => (
          <li
            key={scene.scene}
            className="max-sm:w-64 max-sm:shrink-0 max-sm:snap-start"
          >
            <Link
              href={
                scene.scene === "studio"
                  ? "/storefront"
                  : `/scenes/${scene.scene}`
              }
              className="flex h-full min-h-24 flex-col gap-1 rounded-2xl border bg-card p-4 hover:bg-muted focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
            >
              <span className="flex items-center justify-between gap-2">
                <span className="font-semibold">{title(scene)}</span>
                {scene.saved ? (
                  <span className="text-xs text-muted-foreground">
                    {t("personalised")}
                  </span>
                ) : null}
              </span>
              <span className="text-sm text-muted-foreground">
                {t(`scene.${sceneLayout(scene.scene)}.lead`)}
              </span>
              <span className="mt-auto text-sm font-medium tabular-nums">
                <Signal scene={scene} />
              </span>
            </Link>
          </li>
        ))}
      </ul>
      <p role="status" className="text-sm text-muted-foreground empty:hidden">
        {said}
      </p>
      <div className="flex flex-wrap items-start gap-2">
        {hidden.length ? (
          <details className="text-sm">
            <summary className="min-h-11 cursor-pointer content-center">
              {t("hidden")}
            </summary>
            <ul className="flex flex-col gap-1 pt-1">
              {hidden.map((scene) => (
                <li key={scene.scene} className="flex items-center gap-2">
                  <span>{title(scene)}</span>
                  <Button
                    variant="outline"
                    size="sm"
                    className="min-h-11"
                    aria-label={`${t("restore")} ${title(scene)}`}
                    disabled={update.isPending}
                    onClick={() =>
                      update.mutate({
                        workspace: ws,
                        scene: scene.scene,
                        data: { hidden: false },
                      })
                    }
                  >
                    {t("restore")}
                  </Button>
                </li>
              ))}
            </ul>
          </details>
        ) : null}
        {list.data.data.some((s) => s.saved) ? (
          <ConfirmDialog
            trigger={t("reset")}
            title={t("resetTitle")}
            description={t("resetText")}
            confirm={t("reset")}
            cancel={t("cancel")}
            disabled={reset.isPending}
            onConfirm={() =>
              reset.mutate(
                { workspace: ws },
                { onSuccess: () => say(t("resetDone")) }
              )
            }
          />
        ) : null}
      </div>
    </section>
  )
}
