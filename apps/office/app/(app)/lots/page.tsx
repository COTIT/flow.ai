"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getLotsListQueryKey,
  useLotsCreate,
  useLotsList,
} from "@genesy/api-client/endpoints/lots"
import { useProductsList } from "@genesy/api-client/endpoints/products"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Field, selectClass } from "@/components/field"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

import { LotFields, lotStatuses } from "./lot-fields"

export default function LotsPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string; status?: string; page?: string }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { q, status, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const lots = useLotsList(
    workspaceId,
    {
      q: q || undefined,
      status: status || undefined,
      page: Number(page) || undefined,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  // shortcut: first 100 products only; swap for a search picker when the catalog grows past that.
  const products = useProductsList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const productName = (id: string) =>
    products.data?.data.find((p) => p.id === id)?.name ?? ""
  const go = (next: { q?: string; status?: string; page?: number }) => {
    const params = new URLSearchParams()
    const query = next.q ?? q
    const st = next.status ?? status
    if (query) params.set("q", query)
    if (st) params.set("status", st)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h1 className="mr-auto heading-2 text-lg">{t("lots.title")}</h1>
        <CreateLot
          products={products.data?.data ?? []}
          onCreated={(id) => router.push(`/lots/${id}?done=created`)}
        />
      </div>
      <div className="flex flex-wrap gap-2">
        <form
          role="search"
          onSubmit={(e) => {
            e.preventDefault()
            go({ q: String(new FormData(e.currentTarget).get("q") ?? "") })
          }}
        >
          <Input
            name="q"
            type="search"
            defaultValue={q}
            aria-label={t("lots.search")}
            placeholder={t("lots.search")}
            className="max-w-sm"
          />
        </form>
        <select
          aria-label={t("lots.status")}
          className={selectClass}
          value={status ?? ""}
          onChange={(e) => go({ status: e.target.value })}
        >
          <option value="">{t("lots.allStatuses")}</option>
          {lotStatuses.map((s) => (
            <option key={s} value={s}>
              {t(`lots.statuses.${s}`)}
            </option>
          ))}
        </select>
      </div>
      {lots.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : lots.isError ? (
        <LoadError error={lots.error} onRetry={() => void lots.refetch()} />
      ) : lots.data.data.length === 0 ? (
        <Empty
          title={t("lots.empty")}
          onClear={q || status ? () => router.push("?") : undefined}
        />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("lots.code")}</TableHead>
                <TableHead>{t("lots.product")}</TableHead>
                <TableHead>{t("lots.status")}</TableHead>
                <TableHead>{t("lots.producedOn")}</TableHead>
                <TableHead>{t("lots.bestBefore")}</TableHead>
                <TableHead>{t("lots.publicCode")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {lots.data.data.map((lot) => (
                <TableRow key={lot.id}>
                  <TableCell>
                    <Link
                      className="font-medium underline-offset-4 hover:underline"
                      href={`/lots/${lot.id}`}
                    >
                      {lot.code}
                    </Link>
                  </TableCell>
                  <TableCell>{productName(lot.product_id)}</TableCell>
                  <TableCell>
                    <StatusBadge
                      status={lot.status}
                      label={
                        t.has(`lots.statuses.${lot.status}`)
                          ? t(`lots.statuses.${lot.status}`)
                          : lot.status
                      }
                    />
                  </TableCell>
                  <TableCell>{lot.produced_on}</TableCell>
                  <TableCell>{lot.best_before}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {lot.public_code}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={lots.data.meta.current_page}
            lastPage={lots.data.meta.last_page}
            summary={t("pager.summary", {
              from: lots.data.meta.from ?? 0,
              to: lots.data.meta.to ?? 0,
              total: lots.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={(p) => go({ page: p })}
          />
        </>
      )}
    </section>
  )
}

function CreateLot({
  products,
  onCreated,
}: {
  products: { id: string; name: string }[]
  onCreated: (id: string) => void
}) {
  const t = useTranslations()
  const [open, setOpen] = React.useState(false)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const create = useLotsCreate({
    mutation: {
      onSuccess: ({ data }) => {
        void queryClient.invalidateQueries({
          queryKey: getLotsListQueryKey(workspaceId),
        })
        setOpen(false)
        onCreated(data.id)
      },
    },
  })
  const errors = fieldErrors(create.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) create.reset()
      }}
    >
      <DialogTrigger render={<Button />}>{t("lots.new")}</DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const text = (k: string) => String(f.get(k) ?? "") || null
            create.mutate({
              workspace: workspaceId,
              data: {
                product_id: String(f.get("product_id") ?? ""),
                code: String(f.get("code") ?? ""),
                status: text("status"),
                produced_on: text("produced_on"),
                best_before: text("best_before"),
                expires_on: text("expires_on"),
                origin: text("origin"),
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("lots.new")}</DialogTitle>
          </DialogHeader>
          <Field
            id="lot-product"
            label={t("lots.product")}
            errors={errors.product_id}
          >
            <select name="product_id" className={selectClass} required>
              {products.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
          <LotFields errors={errors} />
          <ApiError error={create.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={create.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
