"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useProductCategoriesList } from "@genesy/api-client/endpoints/product-categories"
import {
  getTaxProfilesGetQueryKey,
  getTaxProfilesListQueryKey,
  useTaxProfilesGet,
  useTaxProfilesVerify,
  useTaxProfilesVerifyReview,
} from "@genesy/api-client/endpoints/tax-profiles"
import type { TaxProfileResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDateTime, formatDay } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import {
  Shell,
  showRate,
  useAction,
  type Outcome,
} from "../../../accounting/shared"
import { placeName, useReasons, useWord } from "../shared"

/** A tax profile (Detail tiles): components, scope, exemptions; verify a draft, correct a verified one. */
export default function TaxProfilePage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = useWord()
  const rights = useRights()
  const queryClient = useQueryClient()
  const { id } = React.use(params)
  const { done: arrived } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const profile = useTaxProfilesGet(workspaceId, id, enabled)
  // shortcut: names for the first 100 categories.
  const categories = useProductCategoriesList(
    workspaceId,
    { page_size: 100 },
    enabled
  )
  const done = useDone(
    arrived === "created" || arrived === "saved"
      ? t(`done.tax.${arrived}`)
      : null
  )
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getTaxProfilesGetQueryKey(workspaceId, id),
      getTaxProfilesListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const outcome: Outcome = {
    done: (message) => {
      setStale(false)
      done.show(message)
      refresh()
    },
    stale: () => {
      done.clear()
      setStale(true)
      refresh()
    },
  }

  if (profile.isPending) return <DetailSkeleton />
  if (profile.isError)
    return (
      <LoadError
        error={profile.error}
        onRetry={() => void profile.refetch()}
        backHref="/settings/taxes"
      />
    )
  const p = profile.data.data
  const category = (cid: string) =>
    categories.data?.data.find((c) => c.product_category_id === cid)?.name ?? ""
  const state = p.superseded_by_id ? "superseded" : p.state
  const mayEdit =
    p.allowed_actions.includes("update") && rights.may("taxprofile.configure")
  const mayVerify =
    p.allowed_actions.includes("verify") && rights.may("taxprofile.verify")
  const mayCorrect =
    p.allowed_actions.includes("correct") && rights.may("taxprofile.configure")

  return (
    <>
      <PageHead
        back={{ href: "/settings/taxes", label: t("taxes.title") }}
        title={t("taxes.profileTitle", {
          place: placeName(p, locale),
          revision: p.revision,
        })}
        description={
          p.effective_to
            ? t("taxes.between", {
                from: formatDay(p.effective_from, locale),
                to: formatDay(p.effective_to, locale),
              })
            : t("taxes.since", { from: formatDay(p.effective_from, locale) })
        }
        actions={
          <StatusBadge
            status={state}
            label={word("states", state)}
            tone={
              state === "verified"
                ? "teal"
                : state === "draft"
                  ? "gold"
                  : "slate"
            }
          />
        }
      />
      {stale ? (
        <Notice variant="warning" title={t("taxes.reasons.stale_document")} />
      ) : null}
      <Bento>
        <Tile
          id="tx-rates"
          title={t("taxes.components")}
          span="col-span-8"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("taxes.name")}</TableHead>
                <TableHead>{t("taxes.code")}</TableHead>
                <TableHead className="text-right">{t("taxes.rate")}</TableHead>
                <TableHead>{t("taxes.recoverable")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {p.components.map((c) => (
                <TableRow key={c.code}>
                  <TableCell className="font-semibold">{c.name}</TableCell>
                  <TableCell data-m="sub">{c.code}</TableCell>
                  <TableCell data-m="right" className="text-right tabular-nums">
                    {showRate(c.rate, locale)}%
                  </TableCell>
                  <TableCell data-m="hide">
                    {t(c.recoverable ? "taxes.yes" : "taxes.no")}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Tile>

        <SideCard id="tx-next" title={t("taxes.nextStep")}>
          <Done message={done.message} />
          {mayVerify ? (
            <>
              <p className="text-[13px]">{t("taxes.next.verify")}</p>
              <VerifyDialog
                profile={p}
                outcome={outcome}
                button={<Button size="lg" className="w-full" />}
              />
            </>
          ) : null}
          {mayEdit ? (
            <Link
              href={`/settings/taxes/${p.id}/edit`}
              className={buttonVariants({
                variant: "outline",
                className: "w-full",
              })}
            >
              {t("taxes.edit")}
            </Link>
          ) : null}
          {mayCorrect ? (
            <>
              <p className="text-[13px]">{t("taxes.next.correct")}</p>
              <Link
                href={`/settings/taxes/new?corrects=${p.id}`}
                className={buttonVariants({
                  variant: "outline",
                  className: "w-full",
                })}
              >
                {t("taxes.correct")}
              </Link>
            </>
          ) : null}
          {!mayVerify && !mayEdit && !mayCorrect ? (
            <Notice
              title={
                p.superseded_by_id
                  ? t("taxes.supersededInfo")
                  : t("taxes.nothingToDo")
              }
            />
          ) : null}
          {p.corrects_id ? (
            <Link
              className="text-[13px] font-medium underline-offset-4 hover:underline"
              href={`/settings/taxes/${p.corrects_id}`}
            >
              {t("taxes.correctsLink")}
            </Link>
          ) : null}
          {p.superseded_by_id ? (
            <Link
              className="text-[13px] font-medium underline-offset-4 hover:underline"
              href={`/settings/taxes/${p.superseded_by_id}`}
            >
              {t("taxes.supersededLink")}
            </Link>
          ) : null}
        </SideCard>

        <Tile id="tx-scope" title={t("taxes.scopeTitle")} span="col-span-6">
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact
              label={t("taxes.appliesTo")}
              value={p.transaction_types
                .map((x) => word("transactionTypes", x))
                .join(", ")}
            />
            <Fact
              label={t("taxes.categories")}
              value={
                p.product_categories.length
                  ? p.product_categories.map(category).join(", ")
                  : t("taxes.allCategories")
              }
            />
            <Fact
              label={t("taxes.taxPoint")}
              value={word("taxPoints", p.tax_point)}
            />
            {p.evidence_ref ? (
              <Fact label={t("taxes.evidence")} value={p.evidence_ref} />
            ) : null}
            {p.verified_at ? (
              <Fact
                label={t("taxes.verifiedAt")}
                value={formatDateTime(p.verified_at, locale)}
              />
            ) : null}
          </SubTile>
        </Tile>

        <Tile id="tx-exempt" title={t("taxes.exemptions")} span="col-span-6">
          {p.exemptions.length === 0 ? (
            <p className="text-[13px] text-muted-foreground">
              {t("taxes.noExemptions")}
            </p>
          ) : (
            <ul className="flex flex-col gap-2 text-[13px]">
              {p.exemptions.map((x, i) => (
                <li key={i}>
                  <span className="block font-semibold">
                    {x.product_category_id
                      ? category(x.product_category_id)
                      : t("taxes.oneCustomer")}
                  </span>
                  <span className="block text-muted-foreground">
                    {x.reason} · {x.evidence_ref}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Tile>
      </Bento>
    </>
  )
}

/** Verify: the server's review of the draft, the evidence, then its fingerprint back. */
function VerifyDialog({
  profile,
  button,
  outcome,
}: {
  profile: TaxProfileResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const review = useTaxProfilesVerifyReview()
  const verify = useTaxProfilesVerify({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.tax.verified"))
      },
    },
  })
  const r = review.data?.data
  const errors = fieldErrors(verify.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        verify.reset()
        review.mutate({ workspace: workspaceId, taxProfile: profile.id })
      }}
      button={button}
      trigger={t("taxes.verify")}
      title={t("taxes.verifyTitle")}
      description={t("taxes.verifyText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (!r) return
          verify.mutate({
            workspace: workspaceId,
            taxProfile: profile.id,
            data: {
              client_ref: a.clientRef,
              expected_version: profile.version,
              payload: {
                review_fingerprint: r.review_fingerprint,
                evidence_ref: String(
                  new FormData(e.currentTarget).get("evidence_ref") ?? ""
                ).trim(),
              },
            },
          })
        }}
      >
        {review.isPending ? (
          <div aria-busy="true" className="flex flex-col gap-2">
            <span className="sr-only">{t("taxes.reviewing")}</span>
            <Skeleton className="h-4 w-2/3" />
          </div>
        ) : r ? (
          <ul className="flex flex-col gap-1 text-[13px]">
            <li className="font-semibold">{placeName(r.profile, locale)}</li>
            {r.profile.components.map((c) => (
              <li key={c.code} className="flex justify-between gap-3">
                <span>{c.name}</span>
                <span className="tabular-nums">
                  {showRate(c.rate, locale)}%
                </span>
              </li>
            ))}
          </ul>
        ) : null}
        <Field
          id="vf-evidence"
          label={t("taxes.evidence")}
          errors={errors["payload.evidence_ref"]}
        >
          <Input
            name="evidence_ref"
            required
            maxLength={255}
            defaultValue={profile.evidence_ref ?? ""}
          />
        </Field>
        <ApiError
          error={review.error ?? verify.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={!r || verify.isPending}>
            {t("taxes.verify")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <p className="flex items-start justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right break-words">{value}</span>
    </p>
  )
}

function SideCard({
  id,
  title,
  children,
}: {
  id: string
  title: string
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className="col-span-4 gap-4 px-6 py-6 max-md:col-span-12 max-md:gap-3 max-md:p-4"
    >
      <h2
        id={id}
        className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
      >
        {title}
      </h2>
      {children}
    </Card>
  )
}
