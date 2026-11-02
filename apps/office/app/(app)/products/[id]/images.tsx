"use client"

import * as React from "react"
import { ArrowDownIcon, ArrowUpIcon, Trash2Icon } from "lucide-react"
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
import { Capped, Empty, LoadError, type useDone } from "@/components/states"
import { selectClass } from "@/components/field"

/**
 * The product's images in display order. Edits stay local until Save, which sends the whole
 * list (the API replaces it); variant images ride along untouched.
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
    <section className="flex flex-col gap-3">
      <h2 className="heading-2 text-base">{t("images.title")}</h2>
      {images.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : images.isError ? (
        <LoadError error={images.error} onRetry={() => void images.refetch()} />
      ) : (
        <>
          {list.length === 0 ? (
            <Empty title={t("images.empty")} />
          ) : (
            <ol className="flex flex-col gap-2">
              {list.map((img, i) => (
                <li
                  key={`${img.file_id}-${i}`}
                  className="flex flex-wrap items-center gap-3 rounded-lg border border-border p-2"
                >
                  {src(img.file_id) ? (
                    <img
                      src={src(img.file_id)}
                      alt=""
                      referrerPolicy="no-referrer"
                      className="size-14 rounded-md bg-muted object-cover"
                    />
                  ) : (
                    <span className="size-14 rounded-md bg-muted" />
                  )}
                  <span className="mr-auto min-w-0 text-sm break-all">
                    {i + 1}. {name(img.file_id)}
                    {img.product_variant_id ? ` · ${t("images.variant")}` : ""}
                    {errors[`images.${i}.file_id`] ? (
                      <span className="block text-destructive">
                        {errors[`images.${i}.file_id`]?.join(" ")}
                      </span>
                    ) : null}
                  </span>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("passport.moveUp")}
                    disabled={i === 0}
                    onClick={() => move(i, i - 1)}
                  >
                    <ArrowUpIcon />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("passport.moveDown")}
                    disabled={i === list.length - 1}
                    onClick={() => move(i, i + 1)}
                  >
                    <ArrowDownIcon />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("images.removeOne", { position: i + 1 })}
                    onClick={() => set(list.filter((_, j) => j !== i))}
                  >
                    <Trash2Icon />
                  </Button>
                </li>
              ))}
            </ol>
          )}
          <Capped meta={files.data?.meta} />
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault()
              const fileId = String(
                new FormData(e.currentTarget).get("file_id") ?? ""
              )
              if (fileId)
                set([...list, { file_id: fileId, product_variant_id: null }])
            }}
          >
            <select
              name="file_id"
              aria-label={t("images.pick")}
              className={selectClass}
            >
              {files.data?.data.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.document_name}
                </option>
              ))}
            </select>
            <Button
              type="submit"
              variant="outline"
              disabled={!files.data?.data.length}
            >
              {t("images.add")}
            </Button>
          </form>
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
    </section>
  )
}
