"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getProductCategoriesListQueryKey,
  useProductCategoriesCreate,
  useProductCategoriesDelete,
  useProductCategoriesList,
  useProductCategoriesUpdate,
} from "@genesy/api-client/endpoints/product-categories"
import type { CategoryResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Textarea } from "@genesy/ui/components/textarea"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Field } from "@/components/field"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

/** Product categories: the shop's category filter and each product's main category. */
export default function CategoriesPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const [page, setPage] = React.useState(1)
  const canCreate = useCan("productcategory.create")
  const canUpdate = useCan("productcategory.update")
  const canDelete = useCan("productcategory.delete")
  const categories = useProductCategoriesList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getProductCategoriesListQueryKey(workspaceId),
    })
  const done = useDone()
  const remove = useProductCategoriesDelete({
    mutation: { onMutate: done.clear, onSuccess: refresh },
  })
  const columns = [
    { label: t("categories.name") },
    { label: t("categories.description") },
    { label: t("categories.products"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.sell")}
        title={t("categories.title")}
        actions={
          <CategoryDialog
            disabled={!canCreate}
            onSaved={() => {
              done.show(t("done.categoryCreated"))
              return refresh()
            }}
          />
        }
      />
      <ApiError error={remove.error} fallback={t("common.error")} />
      <Done message={done.message} />
      <ListCard
        count={
          categories.data
            ? t("categories.count", { total: categories.data.meta.total })
            : null
        }
        pager={
          categories.data?.data.length ? (
            <ListPager meta={categories.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {categories.isPending ? (
          <ListSkeleton columns={columns} />
        ) : categories.isError ? (
          <CardState>
            <LoadError
              error={categories.error}
              onRetry={() => void categories.refetch()}
            />
          </CardState>
        ) : categories.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("categories.empty")} />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((c) => (
                  <TableHead
                    key={c.label}
                    className={c.right ? "text-right" : undefined}
                  >
                    {c.label}
                  </TableHead>
                ))}
                <TableHead>
                  <span className="sr-only">{t("categories.actions")}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {categories.data.data.map((c) => (
                <TableRow key={c.id}>
                  <TableCell className="font-semibold">{c.name}</TableCell>
                  <TableCell data-m="sub" className="text-muted-foreground">
                    {c.description}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right tabular-nums"
                  >
                    {c.products_count}
                  </TableCell>
                  <TableCell data-m="badge" className="text-right">
                    <div className="flex flex-wrap justify-end gap-2">
                      <CategoryDialog
                        category={c}
                        disabled={!canUpdate}
                        onSaved={() => {
                          done.show(t("done.categorySaved"))
                          return refresh()
                        }}
                      />
                      <ConfirmDialog
                        trigger={t("categories.remove")}
                        title={t("categories.removeTitle", { name: c.name })}
                        description={
                          c.products_count > 0
                            ? t("categories.removeUsed", {
                                count: c.products_count,
                              })
                            : t("categories.removeUnused")
                        }
                        confirm={t("categories.remove")}
                        cancel={t("actions.cancel")}
                        disabled={!canDelete || remove.isPending}
                        onConfirm={() =>
                          remove.mutate(
                            { workspace: workspaceId, category: c.id },
                            {
                              onSuccess: () =>
                                done.show(
                                  t("done.categoryRemoved", { name: c.name })
                                ),
                            }
                          )
                        }
                      />
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
    </>
  )
}

/** New category, or with `category` an edit of its name and description. */
function CategoryDialog({
  category,
  disabled,
  onSaved,
}: {
  category?: CategoryResource
  disabled: boolean
  onSaved: () => unknown
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const mutation = { onSuccess: () => (setOpen(false), onSaved()) }
  const create = useProductCategoriesCreate({ mutation })
  const update = useProductCategoriesUpdate({ mutation })
  const save = category ? update : create
  const errors = fieldErrors(save.error)
  const id = category ? `cat-${category.id}` : "cat-new"
  const title = category
    ? t("categories.editTitle", { name: category.name })
    : t("categories.new")

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (!o) save.reset()
      }}
    >
      <DialogTrigger
        disabled={disabled}
        render={
          <Button
            variant={category ? "outline" : "default"}
            size={category ? "sm" : "default"}
          />
        }
      >
        {category ? t("categories.edit") : t("categories.new")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            const data = {
              name: String(f.get("name") ?? ""),
              // An empty description clears it.
              description: String(f.get("description") ?? "") || null,
            }
            if (category)
              update.mutate({
                workspace: workspaceId,
                category: category.id,
                data,
              })
            else create.mutate({ workspace: workspaceId, data })
          }}
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
          <Field
            id={`${id}-name`}
            label={t("categories.name")}
            errors={errors.name}
          >
            <Input name="name" defaultValue={category?.name} required />
          </Field>
          <Field
            id={`${id}-description`}
            label={t("categories.description")}
            errors={errors.description}
          >
            <Textarea
              name="description"
              defaultValue={category?.description ?? ""}
            />
          </Field>
          <ApiError error={save.error} fallback={t("common.error")} />
          <DialogFooter>
            <Button type="submit" disabled={save.isPending}>
              {t("actions.save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
