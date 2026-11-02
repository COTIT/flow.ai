"use client"

import * as React from "react"
import { ArrowLeftIcon, ArrowRightIcon, Trash2Icon } from "lucide-react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { useFilesList } from "@genesy/api-client/endpoints/files"
import {
  getProductsImagesListQueryKey,
  useProductsImagesList,
  useProductsImagesReplace,
} from "@genesy/api-client/endpoints/products"
import type { ImageInput } from "@genesy/api-client/model"
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
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Tile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { Capped, Empty, LoadError, type useDone } from "@/components/states"

/**
 * The product's images in display order. Edits stay local until Save, which sends the whole
 * list (the API replaces it); variant images ride along untouched. "Add image" picks a file in a
 * dialog and only adds it to the draft.
 */
export function ProductImages({
  productId,
  done,
}: {
  productId: string
  done: ReturnType<typeof useDone>
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const images = useProductsImagesList(workspaceId, productId, enabled)
  // shortcut: first 100 files in the picker (warned via Capped); add file search when needed.
  const files = useFilesList(workspaceId, { page_size: 100 }, enabled)
  const [draft, setDraft] = React.useState<ImageInput[] | null>(null)
  const [adding, setAdding] = React.useState(false)
  const replace = useProductsImagesReplace({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.imagesSaved"))
        setDraft(null)
        return queryClient.invalidateQueries({
          queryKey: getProductsImagesListQueryKey(workspaceId, productId),
        })
      },
    },
  })
  const errors = fieldErrors(replace.error)
  const saved: ImageInput[] =
    images.data?.data.map(({ file_id, product_variant_id }) => ({
      file_id,
      product_variant_id,
    })) ?? []
  const list = draft ?? saved
  const set = (next: ImageInput[]) => setDraft(next)
  const move = (from: number, to: number) => {
    const next = [...list]
    next.splice(to, 0, ...next.splice(from, 1))
    set(next)
  }
  // A saved image has a signed link; a file picked just now has its own from the file list.
  const src = (fileId: string) =>
    images.data?.data.find((i) => i.file_id === fileId)?.link?.url ??
    files.data?.data.find((f) => f.id === fileId)?.file_path
  const name = (fileId: string) =>
    files.data?.data.find((f) => f.id === fileId)?.document_name ??
    t("images.unnamed")

  return (
    <Tile
      id="product-images"
      title={t("images.title")}
      span=""
      action={
        <Dialog open={adding} onOpenChange={setAdding}>
          {/* Only on a loaded list: a save after a failed load would drop the saved images. */}
          <DialogTrigger
            render={
              <Button
                variant="outline"
                size="sm"
                disabled={!images.isSuccess || !files.data?.data.length}
              />
            }
          >
            {t("images.add")}
          </DialogTrigger>
          <DialogContent>
            <form
              className="flex flex-col gap-3"
              onSubmit={(e) => {
                e.preventDefault()
                const fileId = String(
                  new FormData(e.currentTarget).get("file_id") ?? ""
                )
                if (fileId)
                  set([...list, { file_id: fileId, product_variant_id: null }])
                setAdding(false)
              }}
            >
              <DialogHeader>
                <DialogTitle>{t("images.add")}</DialogTitle>
              </DialogHeader>
              <Capped meta={files.data?.meta} />
              <Field id="img-file" label={t("images.pick")}>
                <select name="file_id" className={selectClass}>
                  {files.data?.data.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.document_name}
                    </option>
                  ))}
                </select>
              </Field>
              <DialogFooter>
                <Button type="submit">{t("images.add")}</Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      }
    >
      {images.isPending ? (
        <div aria-busy="true" className="grid grid-cols-3 gap-2 max-md:grid-cols-2">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="aspect-square rounded-2xl" />
          ))}
        </div>
      ) : images.isError ? (
        <LoadError error={images.error} onRetry={() => void images.refetch()} />
      ) : (
        <>
          {list.length === 0 ? (
            <Empty title={t("images.empty")} />
          ) : (
            <ol className="grid grid-cols-3 gap-2 max-md:grid-cols-2">
              {list.map((img, i) => (
                <li key={`${img.file_id}-${i}`} className="flex min-w-0 flex-col gap-1">
                  <div className="relative aspect-square overflow-hidden rounded-2xl border border-(--hair) bg-(--glass-subtle)">
                    {src(img.file_id) ? (
                      <img
                        src={src(img.file_id)}
                        alt=""
                        referrerPolicy="no-referrer"
                        className="size-full object-cover"
                      />
                    ) : null}
                    <Button
                      variant="outline"
                      size="icon-sm"
                      className="absolute top-1.5 right-1.5"
                      aria-label={t("images.removeOne", { position: i + 1 })}
                      onClick={() => set(list.filter((_, j) => j !== i))}
                    >
                      <Trash2Icon />
                    </Button>
                    <div className="absolute inset-x-1.5 bottom-1.5 flex justify-between">
                      <Button
                        variant="outline"
                        size="icon-sm"
                        aria-label={t("passport.moveUp")}
                        disabled={i === 0}
                        onClick={() => move(i, i - 1)}
                      >
                        <ArrowLeftIcon />
                      </Button>
                      <Button
                        variant="outline"
                        size="icon-sm"
                        aria-label={t("passport.moveDown")}
                        disabled={i === list.length - 1}
                        onClick={() => move(i, i + 1)}
                      >
                        <ArrowRightIcon />
                      </Button>
                    </div>
                  </div>
                  <p className="truncate text-xs" title={name(img.file_id)}>
                    {i + 1}. {name(img.file_id)}
                  </p>
                  {img.product_variant_id ? (
                    <p className="text-xs text-muted-foreground">
                      {t("images.variant")}
                    </p>
                  ) : null}
                  {errors[`images.${i}.file_id`] ? (
                    <p className="text-xs text-destructive">
                      {errors[`images.${i}.file_id`]?.join(" ")}
                    </p>
                  ) : null}
                </li>
              ))}
            </ol>
          )}
          <p className="text-xs text-muted-foreground">{t("images.hint")}</p>
          <ApiError error={replace.error} fallback={t("common.error")} />
          {draft ? (
            <div className="flex flex-wrap gap-2">
              <Button
                disabled={replace.isPending}
                onClick={() =>
                  replace.mutate({
                    workspace: workspaceId,
                    product: productId,
                    data: { images: draft },
                  })
                }
              >
                {t("images.save")}
              </Button>
              <Button variant="outline" onClick={() => setDraft(null)}>
                {t("images.discard")}
              </Button>
            </div>
          ) : null}
        </>
      )}
    </Tile>
  )
}
