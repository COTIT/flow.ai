"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { Check, Copy, Package, QrCode, Sprout, Truck } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import {
  getLotsEventsQueryKey,
  getLotsGetQueryKey,
  getLotsListQueryKey,
  useLotsEvents,
  useLotsFarmerCredits,
  useLotsGet,
  useLotsPassportGet,
  useLotsUpdate,
} from "@genesy/api-client/endpoints/lots"
import { useProductsGet } from "@genesy/api-client/endpoints/products"
import type { UpdateLotRequest } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { cn } from "@genesy/ui/lib/utils"
import { formatDateTime } from "@genesy/ui/lib/format"
import { Bento, Chip, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { CardState, ListPager, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"

import { LotFields } from "../lot-fields"
import { FarmerCount, FarmerCredits } from "./farmer-credits"
import { PackDialog } from "./pack-dialog"
import { StockDialog } from "./stock-dialog"

const linkClass =
  "font-medium text-foreground underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"

/** The batch detail (OfficeLot): status, passport, lineage, fields, farmer credits, history. */
export default function LotPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  // Set by the page that sent us here (create, receive) so its outcome shows on arrival.
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const { id } = React.use(params)
  const arrived = React.use(searchParams).done
  const done = useDone(
    arrived === "created" || arrived === "received"
      ? t(`done.lot.${arrived}`)
      : null
  )
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const lot = useLotsGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  // Read once the batch is known, so a missing batch makes no further calls.
  const known = { query: { enabled: lot.isSuccess } }
  const product = useProductsGet(
    workspaceId,
    lot.data?.data.product_id ?? "",
    undefined,
    known
  )
  const passport = useLotsPassportGet(workspaceId, id, known)
  // The same first page the Farmer credits card reads (one request): its village groups.
  const credits = useLotsFarmerCredits(workspaceId, id, { page: 1 }, known)
  const groups = credits.data?.farmers ?? []
  const [eventsPage, setEventsPage] = React.useState(1)
  const events = useLotsEvents(
    workspaceId,
    id,
    { page: eventsPage },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const [pending, setPending] = React.useState<UpdateLotRequest | null>(null)
  const [copied, setCopied] = React.useState(false)
  const update = useLotsUpdate({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.lotSaved"))
        for (const queryKey of [
          getLotsGetQueryKey(workspaceId, id),
          getLotsEventsQueryKey(workspaceId, id),
          getLotsListQueryKey(workspaceId),
        ])
          void queryClient.invalidateQueries({ queryKey })
      },
    },
  })
  const save = (data: UpdateLotRequest) =>
    update.mutate({ workspace: workspaceId, lot: id, data })
  const statusLabel = (s: string) =>
    t.has(`lots.statuses.${s}`) ? t(`lots.statuses.${s}`) : s

  if (lot.isPending) return <DetailSkeleton />
  if (lot.isError)
    return (
      <LoadError
        error={lot.error}
        onRetry={() => void lot.refetch()}
        backHref="/lots"
      />
    )
  const current = lot.data.data
  const publicPath = `/b/${current.public_code}`
  const shopUrl = process.env.NEXT_PUBLIC_SHOP_URL
  const pp = passport.data?.data
  const description = [product.data?.data.name, current.origin]
    .filter(Boolean)
    .join(", ")

  return (
    <>
      <PageHead
        back={{ href: "/lots", label: t("lots.title") }}
        title={<span className="font-mono">{current.code}</span>}
        description={description || undefined}
        actions={
          <>
            <PackDialog lotId={id} code={current.code} />
            <StockDialog
              lotId={id}
              code={current.code}
              onChanged={() => done.show(t("done.lotStockChanged"))}
            />
            <Button type="submit" form="lot-form" disabled={update.isPending}>
              {t("actions.save")}
            </Button>
          </>
        }
      />
      <Done message={done.message} />
      <Bento>
        <Tile
          id="lot-status-card"
          title={t("lots.status")}
          span="col-span-4"
          action={
            <StatusBadge
              status={current.status}
              label={statusLabel(current.status)}
            />
          }
        >
          {t.has(`lots.statusText.${current.status}`) ? (
            <p className="text-[13px] text-muted-foreground">
              {t(`lots.statusText.${current.status}`)}
            </p>
          ) : null}
          {current.status !== "recalled" ? (
            <Button
              type="button"
              variant="outline"
              className="border-destructive text-destructive"
              onClick={() => setPending({ status: "recalled" })}
            >
              {t("lots.recallAction")}
            </Button>
          ) : null}
        </Tile>

        <Tile
          id="lot-passport"
          title={t("lots.passport")}
          span="col-span-4"
          action={
            pp ? (
              pp.published ? (
                <Badge data-dot="false">
                  {t("lots.passportPublished", { version: pp.version })}
                </Badge>
              ) : (
                <Badge variant="gold">{t("lots.passportDraft")}</Badge>
              )
            ) : passport.isPending ? (
              <Skeleton className="h-5 w-24 rounded-full" />
            ) : null
          }
        >
          <SubTile className="flex items-center gap-3">
            <QrCode
              aria-hidden="true"
              className="size-6 shrink-0"
              strokeWidth={1.75}
            />
            <div className="min-w-0 grow">
              <p className="font-mono text-sm font-semibold break-all">
                {publicPath}
              </p>
              {pp ? (
                <p className="text-xs text-muted-foreground">
                  {!pp.published
                    ? t("passport.notPublished")
                    : JSON.stringify(pp.draft) === JSON.stringify(pp.published)
                      ? t("lots.draftMatches")
                      : t("passport.draftDiffers")}
                </p>
              ) : null}
            </div>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={copied ? t("lots.linkCopied") : t("lots.copyLink")}
              onClick={() =>
                void navigator.clipboard
                  .writeText(
                    `${shopUrl ?? window.location.origin}${publicPath}`
                  )
                  .then(() => setCopied(true))
                  // No clipboard (denied, insecure page): the path stays readable beside it.
                  .catch(() => setCopied(false))
              }
            >
              {copied ? (
                <Check aria-hidden="true" />
              ) : (
                <Copy aria-hidden="true" />
              )}
            </Button>
          </SubTile>
          {passport.isError ? (
            <LoadError
              error={passport.error}
              onRetry={() => void passport.refetch()}
            />
          ) : null}
          <Link
            href={`/lots/${id}/passport`}
            className={cn(
              buttonVariants(),
              "bg-[image:var(--gold-gradient)] font-semibold text-[#0a183a] hover:opacity-90"
            )}
          >
            {t("lots.editPassport")}
          </Link>
        </Tile>

        <Tile id="lot-lineage" title={t("lots.lineage")} span="col-span-4">
          {current.parent_lot || current.received_from || groups.length ? (
            <div className="flex flex-col gap-2">
              {groups.map((g) => (
                <SubTile
                  key={g.village}
                  className="flex items-center gap-3 px-3 py-2.5"
                >
                  <Chip icon={Sprout} tone="teal" />
                  <div className="min-w-0">
                    <p className="text-sm font-semibold">
                      {t("lineage.harvestFrom", { village: g.village })}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      <FarmerCount farmers={g.farmers} />
                    </p>
                  </div>
                </SubTile>
              ))}
              {current.parent_lot ? (
                <SubTile className="flex items-center gap-3 px-3 py-2.5">
                  <Chip icon={Package} tone="gold" />
                  <div className="min-w-0">
                    <p className="text-sm font-semibold">
                      {t("lineage.packedFrom")}{" "}
                      <Link
                        className={linkClass}
                        href={`/lots/${current.parent_lot.id}`}
                      >
                        {current.parent_lot.code}
                      </Link>
                    </p>
                    {/* The two exact numbers as the API keeps them; no kg-per-jar division here. */}
                    {current.pack ? (
                      <p className="text-xs text-muted-foreground">
                        {t("lineage.pack", {
                          units: current.pack.units,
                          kg: current.pack.raw_kg,
                        })}
                      </p>
                    ) : null}
                  </div>
                </SubTile>
              ) : null}
              {current.received_from ? (
                <SubTile className="flex items-center gap-3 px-3 py-2.5">
                  <Chip icon={Truck} tone="slate" />
                  <div className="min-w-0">
                    <p className="text-sm font-semibold">
                      {t("lineage.receivedFrom", {
                        code: current.received_from.from_lot_code,
                        publicCode: current.received_from.from_lot_public_code,
                      })}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      <Link className={linkClass} href="/supply/transfers">
                        {t("lineage.transfers")}
                      </Link>
                    </p>
                  </div>
                </SubTile>
              ) : null}
            </div>
          ) : (
            <p className="text-[13px] text-muted-foreground">
              {t("lineage.none")}
            </p>
          )}
        </Tile>

        <Tile id="lot-details" title={t("lots.details")} span="col-span-7">
          <form
            id="lot-form"
            key={current.updated_at}
            className="grid gap-4 md:grid-cols-2"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              const text = (k: string) => String(f.get(k) ?? "") || null
              const data = {
                code: text("code"),
                status: text("status"),
                produced_on: text("produced_on"),
                best_before: text("best_before"),
                expires_on: text("expires_on"),
                origin: text("origin"),
              }
              // A recall shows on every jar's public page: ask first.
              if (data.status === "recalled" && current.status !== "recalled")
                setPending(data)
              else save(data)
            }}
          >
            <LotFields lot={current} errors={fieldErrors(update.error)} />
            <div className="empty:hidden md:col-span-2">
              <ApiError error={update.error} fallback={t("common.error")} />
            </div>
          </form>
        </Tile>

        <FarmerCredits lotId={id} />

        <Tile
          id="lot-history"
          title={t("lots.events")}
          span="col-span-12"
          flush
        >
          {events.isPending ? (
            <ListSkeleton
              rows={3}
              columns={[
                { label: t("lots.eventAt") },
                { label: t("lots.eventKind") },
                { label: t("lots.eventWho") },
                { label: t("lots.eventChange") },
              ]}
            />
          ) : events.isError ? (
            <CardState>
              <LoadError
                error={events.error}
                onRetry={() => void events.refetch()}
              />
            </CardState>
          ) : events.data.data.length === 0 ? (
            <CardState>
              <Empty title={t("lots.noEvents")} />
            </CardState>
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("lots.eventAt")}</TableHead>
                    <TableHead>{t("lots.eventKind")}</TableHead>
                    <TableHead>{t("lots.eventWho")}</TableHead>
                    <TableHead>{t("lots.eventChange")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {events.data.data.map((ev) => (
                    <TableRow key={ev.id}>
                      <TableCell className="tabular-nums">
                        {formatDateTime(ev.created_at, locale)}
                      </TableCell>
                      <TableCell data-m="sub">
                        {t(`lots.eventKinds.${ev.kind}`)}
                      </TableCell>
                      {/* No user: the system did it. A user without a name has left. */}
                      <TableCell data-m="hide">
                        {ev.user ? (ev.user.name ?? "—") : t("lots.system")}
                      </TableCell>
                      <TableCell data-m="badge">
                        {ev.from_status || ev.to_status ? (
                          <>
                            {ev.from_status ? (
                              <StatusBadge
                                status={ev.from_status}
                                label={statusLabel(ev.from_status)}
                              />
                            ) : (
                              "—"
                            )}
                            {" → "}
                            {ev.to_status ? (
                              <StatusBadge
                                status={ev.to_status}
                                label={statusLabel(ev.to_status)}
                              />
                            ) : (
                              "—"
                            )}
                          </>
                        ) : null}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              <div className="border-t border-(--hair) px-5 py-3.5 max-md:px-4 max-md:py-3">
                <ListPager meta={events.data.meta} onPage={setEventsPage} />
              </div>
            </>
          )}
        </Tile>
      </Bento>
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(o) => !o && setPending(null)}
        title={t("lots.recallAsk", { code: current.code })}
        description={t("lots.recallText", { code: current.code })}
        confirm={t("lots.recallYes")}
        cancel={t("lots.recallKeep")}
        onConfirm={() => pending && save(pending)}
      />
    </>
  )
}
