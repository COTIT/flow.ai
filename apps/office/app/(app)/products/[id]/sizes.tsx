"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getProductVariantsListQueryKey,
  useProductVariantsGenerate,
  useProductVariantsList,
} from "@genesy/api-client/endpoints/product-variants"
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
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { Empty, LoadError, type useDone } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"
import { useCan } from "@/lib/can"

// The option the shop and the till show a size under ("Size: 250 g"), as the demo catalog uses.
const SIZE = "Size"

/** The product's variants as sizes, with "Add size" for those who may create products. */
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
  const variants = useProductVariantsList(workspaceId, productId, {
    query: { enabled: Boolean(workspaceId) },
  })

  return (
    <Tile
      id="product-sizes"
      title={t("products.sizes")}
      span=""
      action={
        // Always drawn, off without the permission, so the head keeps its size.
        <AddSize
          disabled={!canAdd}
          productId={productId}
          onAdded={() => {
            done.show(t("done.sizeAdded"))
            void queryClient.invalidateQueries({
              queryKey: getProductVariantsListQueryKey(workspaceId, productId),
            })
          }}
        />
      }
    >
      {variants.isPending ? (
        <div aria-busy="true" className="flex flex-col gap-2">
          {/* One size row's height (badge included), so the cards below stay put. */}
          <Skeleton className="h-6" />
        </div>
      ) : variants.isError ? (
        <LoadError
          error={variants.error}
          onRetry={() => void variants.refetch()}
        />
      ) : variants.data.data.length === 0 ? (
        <Empty title={t("products.noSizes")} />
      ) : (
        <div className="flex flex-col gap-2 text-sm">
          {variants.data.data.map((v) => (
            <div
              key={v.id}
              className="flex min-h-6 items-center justify-between gap-3"
            >
              <span className="flex min-w-0 flex-wrap items-center gap-2">
                <span className="font-semibold">
                  {v.relationships.option_values
                    .map((o) => o.display_value || o.value)
                    .join(", ") || t("products.standardSize")}
                </span>
                {v.sku ? (
                  <span className="font-mono text-muted-foreground">
                    {v.sku}
                  </span>
                ) : null}
                {v.is_default ? (
                  <StatusBadge
                    status="default"
                    tone="slate"
                    label={t("products.defaultSize")}
                  />
                ) : null}
              </span>
              {/* Net price as the API prints it, never parsed. */}
              <span className="tabular-nums">{v.price}</span>
            </div>
          ))}
        </div>
      )}
      <p className="mt-3 text-xs text-muted-foreground">{t("products.sizesShopNote")}</p>
    </Tile>
  )
}

function AddSize({
  disabled,
  productId,
  onAdded,
}: {
  disabled: boolean
  productId: string
  onAdded: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const generate = useProductVariantsGenerate({
    mutation: {
      onSuccess: () => {
        onAdded()
        setOpen(false)
      },
    },
  })
  // The API names the nested key (`option_types.0.values.0.value`, `combinations.0.price`).
  const errors = Object.entries(fieldErrors(generate.error))
  const errorsFor = (end: string) =>
    errors.flatMap(([k, v]) => (k.endsWith(end) ? v : []))

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) generate.reset()
      }}
    >
      <DialogTrigger
        render={<Button variant="outline" size="sm" disabled={disabled} />}
      >
        {t("products.addSize")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const value = String(f.get("size") ?? "")
            const sku = String(f.get("sku") ?? "")
            generate.mutate({
              workspace: workspaceId,
              product: productId,
              data: {
                option_types: [{ name: SIZE, values: [{ value }] }],
                combinations: [
                  {
                    option_value_ids: [value],
                    price: String(f.get("price") ?? ""),
                    ...(sku ? { sku } : {}),
                  },
                ],
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("products.addSize")}</DialogTitle>
          </DialogHeader>
          <Field
            id="ns-size"
            label={t("products.size")}
            errors={errorsFor(".value")}
          >
            <Input name="size" required />
          </Field>
          <Field
            id="ns-price"
            label={t("products.netPrice")}
            errors={errorsFor(".price")}
          >
            <Input name="price" inputMode="decimal" required />
          </Field>
          <Field
            id="ns-sku"
            label={t("products.sku")}
            errors={errorsFor(".sku")}
          >
            <Input name="sku" className="font-mono" />
          </Field>
          <ApiError error={generate.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={generate.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
