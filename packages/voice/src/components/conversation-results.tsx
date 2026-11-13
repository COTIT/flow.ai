"use client"

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@genesy/ui/components/button"
import { Checkbox } from "@genesy/ui/components/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { fieldName, type TranscriptRow } from "../lib/helpers"
import {
  answerResultIds,
  latestChart,
  reportItems,
  reportPeriod,
  reportRows,
  retainedSelection,
  type ReportItem,
  type ReportLabels,
  type ReportRow,
} from "../lib/report"
import { drawnAs, type ResultView } from "../lib/views"
import { VoiceResult } from "./results/index"
import { ReportChart } from "./results/kinds"

type Selection = {
  selected: string[]
  setSelected: Dispatch<SetStateAction<string[]>>
  offered: Set<string>
}
const ReportSelection = createContext<Selection | null>(null)

/** One selection shared by inline widgets and the conversation's report toolbar. */
export function ReportSelectionProvider({ children }: { children: ReactNode }) {
  const [selected, setSelected] = useState<string[]>([])
  const [offered] = useState(() => new Set<string>())
  const value = useMemo(
    () => ({ selected, setSelected, offered }),
    [selected, offered]
  )
  return (
    <ReportSelection.Provider value={value}>
      {children}
    </ReportSelection.Provider>
  )
}

export function ResultSelection({ row }: { row: TranscriptRow }) {
  const t = useTranslations("voice.reports")
  const selection = useContext(ReportSelection)
  if (!selection || row.role !== "result" || !row.views?.length) return null
  const included = selection.selected.includes(row.id)
  return (
    <Button
      variant="ghost"
      size="sm"
      className="min-h-11 self-start"
      data-report-choice={row.id}
      aria-pressed={included}
      onClick={() =>
        selection.setSelected((ids) =>
          included ? ids.filter((id) => id !== row.id) : [...ids, row.id]
        )
      }
    >
      {t(included ? "remove" : "add")}
    </Button>
  )
}

const frame = () =>
  new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))

/** Include fitting panes too: incoming results can make them scroll during a modal. */
export function captureScroll() {
  const elements = new Set<HTMLElement>()
  for (const pane of document.querySelectorAll<HTMLElement>(
    '[data-flow-scroll], [data-flow-pane], [role="log"]'
  )) {
    for (
      let element: HTMLElement | null = pane;
      element;
      element = element.parentElement
    )
      elements.add(element)
  }
  if (document.scrollingElement instanceof HTMLElement)
    elements.add(document.scrollingElement)
  const positions = [...elements].map((element) => ({
    element,
    top: element.scrollTop,
    left: element.scrollLeft,
  }))
  return () => {
    for (const { element, top, left } of positions) {
      if (!element.isConnected) continue
      element.scrollTop = top
      element.scrollLeft = left
    }
  }
}
const PLOTS = new Set(["line", "sparkline", "bar", "hbar", "pie", "donut"])
function hasPlotData(view: ResultView): boolean {
  if (!PLOTS.has(drawnAs(view))) return false
  switch (view.kind) {
    case "sales":
      return Boolean(view.rows.length)
    case "sales_daily":
      return view.currencies.some((c) => c.days.length)
    case "comparison":
      return Boolean(view.currencies.length)
    case "breakdown":
      return view.currencies.some((c) => c.rows.length)
    case "stock":
      return view.groups.some((g) => g.rows.length)
    case "lot_origin":
      return Boolean(view.materials.length)
    case "attention":
      return view.sections.some((s) => Object.keys(s.counts).length)
    default:
      return false
  }
}

/** Local download only: no report data is sent to another service or stored persistently. */
function save(blob: Blob, filename: string) {
  const href = URL.createObjectURL(blob)
  const link = document.createElement("a")
  link.href = href
  link.download = filename
  document.body.append(link)
  link.click()
  link.remove()
  setTimeout(() => URL.revokeObjectURL(href), 1000)
}

/** Persistent access to chart snapshots and exports; the transcript never moves to open either. */
export function ConversationResults({
  rows,
  completedRequestId,
  reportOpen,
  onReportOpenChange,
}: {
  rows: TranscriptRow[]
  completedRequestId: string | null
  reportOpen: boolean
  onReportOpenChange: (open: boolean) => void
}) {
  const t = useTranslations("voice.reports")
  const result = useTranslations("voice.results")
  const locale = useLocale()
  const selection = useContext(ReportSelection)
  const available = reportRows(rows)
  const latest = latestChart(rows)
  const [preview, setPreview] = useState<ReportRow | null>(null)
  const [dismissedOffer, setDismissedOffer] = useState<string | null>(null)
  const [evicted, setEvicted] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState(false)
  const [exporting, setExporting] = useState<ReportItem[]>([])
  const exportRoot = useRef<HTMLDivElement>(null)
  const latestButton = useRef<HTMLButtonElement>(null)
  const reportButton = useRef<HTMLButtonElement>(null)
  const previewScroll = useRef<(() => void) | null>(null)
  const reportScroll = useRef<(() => void) | null>(null)
  const reportWasOpen = useRef(false)
  const rowsKey = available.map((row) => row.id).join("\n")
  const [seenRows, setSeenRows] = useState(rowsKey)
  if (seenRows !== rowsKey) {
    setSeenRows(rowsKey)
    if (!available.length) {
      setPreview(null)
      setDismissedOffer(null)
      setEvicted(false)
      setError(false)
    } else if (
      selection &&
      retainedSelection(selection.selected, rows).length !==
        selection.selected.length
    ) {
      setEvicted(true)
    }
  }
  const offer =
    completedRequestId &&
    completedRequestId !== dismissedOffer &&
    !selection?.offered.has(completedRequestId) &&
    answerResultIds(rows, completedRequestId).length >= 2
      ? completedRequestId
      : null

  useEffect(() => {
    if (!selection) return
    const next = retainedSelection(selection.selected, rows)
    if (next.length !== selection.selected.length) {
      selection.setSelected(next)
    }
    if (!available.length) {
      selection.offered.clear()
      onReportOpenChange(false)
    }
    // Reconcile only when the retained IDs change; selection changes never evict anything.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rowsKey])

  useEffect(() => {
    if (reportOpen) {
      if (!reportWasOpen.current) reportScroll.current ??= captureScroll()
      if (completedRequestId) selection?.offered.add(completedRequestId)
    } else if (reportWasOpen.current) {
      reportScroll.current?.()
    }
    reportWasOpen.current = reportOpen
  }, [reportOpen, completedRequestId, selection?.offered])

  const title = (view: ResultView) => {
    if ("title" in view && view.title) return view.title
    if ("resource" in view) return result(`resources.${view.resource}`)
    switch (view.kind) {
      case "sales":
        return result(
          `source.${view.source && result.has(`source.${view.source}`) ? view.source : "all"}`
        )
      case "sales_daily":
        return result("source.all")
      case "breakdown":
        return result(`breakdown.${view.by}`)
      case "comparison":
        return result("compare.title")
      case "stock":
        return result(view.scope === "bins" ? "stock.perBin" : "stock.title")
      case "lot_origin":
        return result("originTitle")
      case "attention":
        return result("attentionTitle")
      case "trail":
        return result("trailTitle")
      case "search":
        return result("search.title")
      case "till_sales":
        return result("till.sales")
      case "till_shifts":
        return result("till.shifts")
      default:
        return t("widget")
    }
  }
  const labels: ReportLabels = {
    locale,
    minorUnits: t("minorUnits"),
    title: t("title"),
    snapshot: t("snapshot"),
    captured: t("captured"),
    widget: t("widget"),
    period: t("period"),
    columns: (key) => {
      const text = key
        .split(".")
        .map((part) =>
          result.has(`facts.${part}`)
            ? result(`facts.${part}`)
            : fieldName(part)
        )
        .join(" · ")
      return key.endsWith("minor") ||
        ["gross", "net", "vat", "refunded", "current", "base"].includes(key)
        ? `${text} (${t("minorUnits")})`
        : key === "share" || key === "change_bp"
          ? `${text} (${t("basisPoints")})`
          : text
    },
  }

  const download = async (format: "pdf" | "xlsx") => {
    if (!selection || busy) return
    const items = reportItems(rows, selection.selected, title)
    if (!items.length) return
    setBusy(true)
    setError(false)
    try {
      const filename = `genesy-report-${new Date().toISOString().slice(0, 10)}.${format}`
      if (format === "xlsx") {
        const { reportWorkbook } = await import("../lib/report-excel")
        const bytes = await reportWorkbook(items, labels)
        save(
          new Blob([bytes as BlobPart], {
            type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
          }),
          filename
        )
      } else {
        setExporting(items)
        const chartItems = items.filter((item) =>
          item.row.views?.some(hasPlotData)
        )
        // Recharts registers its layout after mount, even with fixed export dimensions.
        for (let attempt = 0; attempt < 120; attempt++) {
          await frame()
          if (
            chartItems.every((item) =>
              exportRoot.current?.querySelector(
                `[data-report-render="${item.row.id}"] svg.recharts-surface`
              )
            )
          )
            break
          if (attempt === 119) throw new Error("Report chart could not render")
        }
        await frame()
        await frame()
        const charts = new Map(
          items.map((item) => [
            item.row.id,
            [
              ...(exportRoot.current
                ?.querySelector(`[data-report-render="${item.row.id}"]`)
                ?.querySelectorAll<SVGSVGElement>("svg.recharts-surface") ??
                []),
            ],
          ])
        )
        const { reportPdf } = await import("../lib/report-pdf")
        save(await reportPdf(items, charts, labels), filename)
      }
    } catch {
      setError(true)
    } finally {
      setBusy(false)
      setExporting([])
    }
  }

  if (!selection || !available.length) return null
  return (
    <div data-conversation-results className="flex min-w-0 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Dialog
          open={Boolean(preview)}
          onOpenChange={(open) => {
            if (!open) setPreview(null)
          }}
        >
          <DialogTrigger
            render={
              <Button
                ref={latestButton}
                variant="outline"
                className="min-h-11"
                disabled={!latest}
              />
            }
            onClick={() => {
              if (latest) {
                previewScroll.current = captureScroll()
                setPreview(latest)
              }
            }}
          >
            {t("latest")}
          </DialogTrigger>
          <DialogContent
            data-chart-preview
            className="max-h-[90dvh] overflow-y-auto sm:max-w-5xl"
            finalFocus={() => {
              previewScroll.current?.()
              previewScroll.current = null
              latestButton.current?.focus({ preventScroll: true })
              return false
            }}
          >
            <DialogTitle>{t("latest")}</DialogTitle>
            <DialogDescription>{t("snapshot")}</DialogDescription>
            {preview ? <VoiceResult views={preview.views ?? []} /> : null}
          </DialogContent>
        </Dialog>
        <Dialog open={reportOpen} onOpenChange={onReportOpenChange}>
          <DialogTrigger
            onClick={() => {
              reportScroll.current = captureScroll()
            }}
            render={
              <Button
                ref={reportButton}
                variant="outline"
                className="min-h-11"
              />
            }
          >
            {t("report", { count: selection.selected.length })}
          </DialogTrigger>
          <DialogContent
            data-report-picker
            className="max-h-[90dvh] overflow-y-auto sm:max-w-3xl"
            finalFocus={() => {
              reportScroll.current?.()
              reportScroll.current = null
              reportButton.current?.focus({ preventScroll: true })
              return false
            }}
          >
            <DialogTitle>{t("title")}</DialogTitle>
            <DialogDescription>{t("snapshot")}</DialogDescription>
            {evicted ? (
              <p role="status" className="text-sm text-muted-foreground">
                {t("evicted")}
              </p>
            ) : null}
            <ul className="flex min-w-0 flex-col gap-2">
              {[...available].reverse().map((row) => (
                <li key={row.id}>
                  <label className="flex min-h-12 cursor-pointer items-center gap-3 rounded-xl border p-3">
                    <Checkbox
                      checked={selection.selected.includes(row.id)}
                      disabled={busy}
                      onCheckedChange={(checked) =>
                        selection.setSelected((ids) =>
                          checked
                            ? [...ids.filter((id) => id !== row.id), row.id]
                            : ids.filter((id) => id !== row.id)
                        )
                      }
                    />
                    <span className="flex min-w-0 flex-col">
                      <span className="font-medium break-words">
                        {title(row.views![0]!)}
                      </span>
                      {reportPeriod(row) ? (
                        <span className="text-xs text-muted-foreground">
                          {reportPeriod(row)}
                        </span>
                      ) : null}
                      {row.createdAt ? (
                        <span className="text-xs text-muted-foreground">
                          {t("captured")}:{" "}
                          {new Intl.DateTimeFormat(locale, {
                            dateStyle: "medium",
                            timeStyle: "short",
                          }).format(new Date(row.createdAt))}
                        </span>
                      ) : null}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
            {error ? (
              <p role="alert" className="text-sm text-destructive">
                {t("error")}
              </p>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <Button
                className="min-h-11"
                disabled={busy || !selection.selected.length}
                onClick={() => void download("pdf")}
              >
                {t("pdf")}
              </Button>
              <Button
                className="min-h-11"
                variant="outline"
                disabled={busy || !selection.selected.length}
                onClick={() => void download("xlsx")}
              >
                {t("excel")}
              </Button>
            </div>
            {busy ? (
              <p role="status" className="text-sm">
                {t("preparing")}
              </p>
            ) : null}
          </DialogContent>
        </Dialog>
      </div>
      {offer && !reportOpen ? (
        <div
          data-report-offer
          className="flex flex-wrap items-center gap-2 text-sm"
        >
          <p>{t("offer")}</p>
          <Button
            variant="outline"
            className="min-h-11"
            onClick={() => {
              reportScroll.current = captureScroll()
              selection.setSelected(answerResultIds(rows, offer))
              selection.offered.add(offer)
              setDismissedOffer(offer)
              onReportOpenChange(true)
            }}
          >
            {t("prepare")}
          </Button>
          <Button
            variant="ghost"
            className="min-h-11"
            onClick={() => {
              selection.offered.add(offer)
              setDismissedOffer(offer)
            }}
          >
            {t("dismiss")}
          </Button>
        </div>
      ) : null}
      {exporting.length ? (
        <div
          ref={exportRoot}
          aria-hidden
          inert
          style={{
            position: "fixed",
            left: -10000,
            top: 0,
            width: 720,
            color: "#1c1b18",
            background: "#fff",
            pointerEvents: "none",
          }}
        >
          {exporting.map((item) => (
            <div key={item.row.id} data-report-render={item.row.id}>
              <ReportChart
                views={item.row.views ?? []}
                width={720}
                height={320}
              />
            </div>
          ))}
        </div>
      ) : null}
    </div>
  )
}
