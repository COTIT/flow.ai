"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getProductsListQueryKey,
  useProductsCreate,
} from "@genesy/api-client/endpoints/products"
import { useVatGroupsList } from "@genesy/api-client/endpoints/vat-groups"
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
import { Notice } from "@genesy/ui/components/notice"
import { Field, selectClass } from "@/components/field"
import { Capped } from "@/components/states"

/**
 * "New product": only what makes it sellable (name, net price, VAT group). The API fills the
 * rest (active, physical, stock-tracked, default currency and VAT group), so the till and the
 * shop list it at once; the product page edits everything else.
 */
export function CreateProduct({
  startOpen = false,
  onCreated,
}: {
  startOpen?: boolean
  onCreated: (id: string) => void
}) {
  const t = useTranslations()
  const [open, setOpen] = React.useState(startOpen)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const vatGroups = useVatGroupsList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: open && Boolean(workspaceId) } }
  )
  const create = useProductsCreate({
    mutation: {
      onSuccess: ({ data }) => {
        void queryClient.invalidateQueries({
          queryKey: getProductsListQueryKey(workspaceId),
        })
        setOpen(false)
        onCreated(data.product_id)
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
      <DialogTrigger render={<Button />}>{t("products.new")}</DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const vat = String(f.get("vat_group_id") ?? "")
            create.mutate({
              workspace: workspaceId,
              data: {
                name: String(f.get("name") ?? ""),
                // Sent as typed; the API checks it.
                price: String(f.get("price") ?? ""),
                // Left out, the API takes the workspace's default group.
                ...(vat ? { vat_group_id: vat } : {}),
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("products.new")}</DialogTitle>
          </DialogHeader>
          <Field id="np-name" label={t("products.name")} errors={errors.name}>
            <Input name="name" required />
          </Field>
          <Field
            id="np-price"
            label={t("products.netPrice")}
            errors={errors.price}
          >
            <Input name="price" inputMode="decimal" required />
          </Field>
          <Field
            id="np-vat"
            label={t("products.vatGroup")}
            errors={errors.vat_group_id}
          >
            <select name="vat_group_id" className={selectClass}>
              <option value="">{t("products.vatDefault")}</option>
              {vatGroups.data?.data.map((g) => (
                <option key={g.id} value={g.id}>
                  {`${g.name} · ${g.vat_amount} %`}
                </option>
              ))}
            </select>
          </Field>
          {vatGroups.isError ? (
            <Notice variant="warning" title={t("products.vatFailed")} />
          ) : vatGroups.data?.data.length === 0 ? (
            <Notice title={t("products.vatNone")} />
          ) : (
            <Capped meta={vatGroups.data?.meta} />
          )}
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
