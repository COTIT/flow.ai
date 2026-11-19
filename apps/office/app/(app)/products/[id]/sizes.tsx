"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@verzo/api-client/auth"
import {
  getProductVariantsListQueryKey,
  useProductVariantsDelete,
  useProductVariantsGenerate,
  useProductVariantsList,
  useProductVariantsUpdate,
} from "@verzo/api-client/endpoints/product-variants"
import type { ProductVariantResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { ConfirmDialog } from "@verzo/ui/components/confirm-dialog"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Skeleton } from "@verzo/ui/components/skeleton"
import { Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { ListPager } from "@/components/list"
import { Empty, LoadError, type useDone } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"
import { useCan } from "@/lib/can"

// The option the shop and the till show a size under ("Size: 250 g"), as the demo catalog uses.
const SIZE = "Size"
const PAGE_SIZE = 5

function sizeLabels(variant: ProductVariantResource) {
  return variant.relationships.option_values
    .filter((option) => {
      const type = option.relationships?.option_type
      return (
        typeof type === "object" &&
        type !== null &&
        "name" in type &&
        type.name === "size"
      )
    })
    .map((option) => option.display_value || option.value)
}

/** Search and manage the product's variants. */
export function ProductSizes({
  productId,
  done,
}: {
  productId: string
  done: ReturnType<typeof useDone>
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const canAdd = useCan("product.create")
  const canEdit = useCan("product.update")
  const canRemove = useCan("product.delete")
  const [search, setSearch] = React.useState("")
  const [page, setPage] = React.useState(1)
  const variants = useProductVariantsList(workspaceId, productId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getProductVariantsListQueryKey(workspaceId, productId),
    })
  const remove = useProductVariantsDelete({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.sizeRemoved"))
        return refresh()
      },
    },
  })
  const rows = (variants.data?.data ?? []).map((variant) => ({
    variant,
    label:
      variant.relationships.option_values
        .map((option) => option.display_value || option.value)
        .join(", ") || t("products.standardSize"),
  }))
  const term = search.trim().toLowerCase()
  const filtered = rows.filter(
    ({ label, variant }) =>
      label.toLowerCase().includes(term) ||
      variant.sku?.toLowerCase().includes(term)
  )
  const lastPage = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const currentPage = Math.min(page, lastPage)
  if (variants.isSuccess && page !== currentPage) setPage(currentPage)
  const offset = (currentPage - 1) * PAGE_SIZE

  return (
    <Tile
      id="product-sizes"
      title={t("products.sizes")}
      span=""
      action={
        // Always drawn, off without the permission, so the head keeps its size.
        <SizeDialog
          disabled={!canAdd}
          productId={productId}
          onSaved={() => {
            done.show(t("done.sizeAdded"))
            void refresh()
          }}
        />
      }
    >
      <Input
        type="search"
        aria-label={t("products.searchSizes")}
        placeholder={t("products.searchSizes")}
        className="mb-3"
        value={search}
        disabled={rows.length === 0}
        onChange={(event) => {
          setSearch(event.target.value)
          setPage(1)
        }}
      />
      {variants.isPending ? (
        <div aria-busy="true" className="flex flex-col gap-2">
          <Skeleton className="h-11 md:h-8" />
        </div>
      ) : variants.isError ? (
        <LoadError
          error={variants.error}
          onRetry={() => void variants.refetch()}
        />
      ) : variants.data.data.length === 0 ? (
        <Empty title={t("products.noSizes")} />
      ) : (
        <div className="flex flex-col gap-3 text-sm">
          {filtered.length === 0 ? (
            <Empty title={t("products.noMatchingSizes")} />
          ) : (
            <ul className="flex flex-col gap-2">
              {filtered
                .slice(offset, offset + PAGE_SIZE)
                .map(({ variant, label }) => (
                  <li
                    key={variant.id}
                    className="flex min-h-11 flex-wrap items-center justify-between gap-x-3 gap-y-1 md:min-h-8"
                  >
                    <span className="flex min-w-0 flex-wrap items-center gap-2">
                      <span className="font-semibold break-all">{label}</span>
                      {variant.sku ? (
                        <span className="font-mono break-all text-muted-foreground">
                          {variant.sku}
                        </span>
                      ) : null}
                      {variant.is_default ? (
                        <StatusBadge
                          status="default"
                          tone="slate"
                          label={t("products.defaultSize")}
                        />
                      ) : null}
                    </span>
                    <span className="ml-auto flex flex-wrap items-center justify-end gap-2">
                      <span className="tabular-nums">{variant.price}</span>
                      {canEdit ? (
                        <SizeDialog
                          productId={productId}
                          variant={variant}
                          label={label}
                          onSaved={() => {
                            done.show(t("done.sizeSaved"))
                            void refresh()
                          }}
                        />
                      ) : null}
                      {canRemove && !variant.is_default ? (
                        <ConfirmDialog
                          trigger={t("products.removeSize")}
                          title={t("products.removeSizeTitle", { label })}
                          description={t("products.removeSizeDescription")}
                          confirm={t("products.removeSize")}
                          cancel={t("actions.cancel")}
                          disabled={remove.isPending}
                          onConfirm={() =>
                            remove.mutate({
                              workspace: workspaceId,
                              product: productId,
                              variant: variant.id,
                            })
                          }
                        />
                      ) : null}
                    </span>
                  </li>
                ))}
            </ul>
          )}
          {lastPage > 1 ? (
            <ListPager
              meta={{
                current_page: currentPage,
                last_page: lastPage,
                from: offset + 1,
                to: Math.min(offset + PAGE_SIZE, filtered.length),
                total: filtered.length,
              }}
              onPage={setPage}
            />
          ) : null}
        </div>
      )}
      <ApiError error={remove.error} fallback={t("common.error")} />
      <p className="mt-3 text-xs text-muted-foreground">
        {t("products.sizesShopNote")}
      </p>
    </Tile>
  )
}

function SizeDialog({
  disabled,
  productId,
  variant,
  label,
  onSaved,
}: {
  disabled?: boolean
  productId: string
  variant?: ProductVariantResource
  label?: string
  onSaved: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const inputId = React.useId()
  const [open, setOpen] = React.useState(false)
  const [initial, setInitial] = React.useState(variant)
  const onSuccess = () => {
    setOpen(false)
    onSaved()
  }
  const generate = useProductVariantsGenerate({ mutation: { onSuccess } })
  const update = useProductVariantsUpdate({ mutation: { onSuccess } })
  const mutation = variant ? update : generate
  const labels = initial ? sizeLabels(initial) : []
  const title = t(variant ? "products.editSize" : "products.addSize")
  // The API names the nested key (`option_types.0.values.0.value`, `combinations.0.price`).
  const errors = fieldErrors(mutation.error)
  const errorsFor = (end: string) =>
    Object.entries(errors).flatMap(([k, v]) => (k.endsWith(end) ? v : []))

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) setInitial(variant)
        else mutation.reset()
      }}
    >
      <DialogTrigger
        render={<Button variant="outline" size="sm" disabled={disabled} />}
        aria-label={
          variant && label ? t("products.editSizeNamed", { label }) : undefined
        }
      >
        {title}
      </DialogTrigger>
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto">
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const values = f.getAll("size").map((value) => String(value).trim())
            const value = values[0] ?? ""
            const price = String(f.get("price") ?? "").trim()
            const sku = String(f.get("sku") ?? "").trim()
            if (initial) {
              const data = {
                ...(values.some((text, index) => text !== (labels[index] ?? ""))
                  ? { size_labels: values }
                  : {}),
                ...(price !== initial.price ? { price } : {}),
                ...(sku !== (initial.sku ?? "") ? { sku: sku || null } : {}),
              }
              if (Object.keys(data).length === 0) {
                setOpen(false)
                return
              }
              update.mutate({
                workspace: workspaceId,
                product: productId,
                variant: initial.id,
                data,
              })
              return
            }
            generate.mutate({
              workspace: workspaceId,
              product: productId,
              data: {
                option_types: [{ name: SIZE, values: [{ value }] }],
                combinations: [
                  {
                    option_value_ids: [value],
                    price,
                    ...(sku ? { sku } : {}),
                  },
                ],
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
          {(labels.length ? labels : [""]).map((label, index) => (
            <Field
              key={index}
              id={`${inputId}-size-${index}`}
              label={
                labels.length > 1
                  ? t("products.numberedSize", { number: index + 1 })
                  : t("products.size")
              }
              errors={
                variant
                  ? [
                      ...(errors[`size_labels.${index}`] ?? []),
                      ...(index === 0 ? (errors.size_labels ?? []) : []),
                    ]
                  : errorsFor(".value")
              }
            >
              <Input
                name="size"
                defaultValue={label}
                required={!variant || labels.length > 0}
              />
            </Field>
          ))}
          <Field
            id={`${inputId}-price`}
            label={t("products.netPrice")}
            errors={variant ? errors.price : errorsFor(".price")}
          >
            <Input
              name="price"
              inputMode="decimal"
              required
              defaultValue={initial?.price}
            />
          </Field>
          <Field
            id={`${inputId}-sku`}
            label={t("products.sku")}
            errors={variant ? errors.sku : errorsFor(".sku")}
          >
            <Input
              name="sku"
              className="font-mono"
              defaultValue={initial?.sku ?? ""}
            />
          </Field>
          <ApiError error={mutation.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={mutation.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
