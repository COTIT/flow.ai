"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { sessionStore } from "@genesy/api-client/session"
import {
  getStorefrontConnectionGetQueryKey,
  useStorefrontConnectionGet,
  useStorefrontConnectionUpdate,
} from "@genesy/api-client/endpoints/storefront-connection"
import { ApiError } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { Done, LoadError, useDone } from "@/components/states"
import { storefrontUrl } from "@/lib/storefront-url"

/** Mounted inside the admin-only, workspace/login-keyed Storefront settings page. */
export function StorefrontConnection({
  workspaceId,
  token,
}: {
  workspaceId: string
  token: string
}) {
  const t = useTranslations("storefront.connection")
  const common = useTranslations()
  const queryClient = useQueryClient()
  const done = useDone()
  const connection = useStorefrontConnectionGet(workspaceId, {
    query: { retry: false },
  })
  const [disconnect, setDisconnect] = React.useState<string | null>(null)
  const [copied, setCopied] = React.useState(false)
  const [copyFailed, setCopyFailed] = React.useState(false)
  const sameSession = () => {
    const session = sessionStore.getState()
    return session.workspaceId === workspaceId && session.token === token
  }
  const update = useStorefrontConnectionUpdate({
    mutation: {
      onMutate: () => {
        done.clear()
        setCopied(false)
        setCopyFailed(false)
      },
      onSuccess: (response) => {
        if (!sameSession()) return
        queryClient.setQueryData(
          getStorefrontConnectionGetQueryKey(workspaceId),
          response
        )
        done.show(
          response.data.active ? t("connectedDone") : t("disconnectedDone")
        )
        return queryClient.invalidateQueries({
          queryKey: getStorefrontConnectionGetQueryKey(workspaceId),
        })
      },
    },
  })
  const current = connection.data?.data
  const busy = update.isPending || connection.isFetching
  const ready = Boolean(current && !connection.isError && !busy)
  const url =
    !connection.isError && !busy
      ? storefrontUrl(process.env.NEXT_PUBLIC_SHOP_URL, current)
      : null
  const change = (active: boolean) => {
    if (!current) return
    if (
      !ready ||
      !sameSession() ||
      (active ? current.active : current.id !== disconnect || !current.active)
    )
      return
    update.mutate({ workspace: workspaceId, data: { active } })
  }

  return (
    <Tile id="storefront-connection" title={t("title")} span="col-span-12">
      <Done message={done.message} />
      {connection.isPending ? (
        <Skeleton
          className="h-20 w-full"
          aria-label={common("common.loading")}
        />
      ) : connection.isError ? (
        <LoadError
          error={connection.error}
          onRetry={() => void connection.refetch()}
        />
      ) : current ? (
        <>
          <div className="flex flex-wrap items-center gap-3">
            <Badge variant={current.active ? "teal" : "slate"}>
              {current.active ? t("connected") : t("disconnected")}
            </Badge>
            <p className="text-sm">{current.name}</p>
          </div>
          <p className="max-w-prose text-sm text-muted-foreground">
            {current.active ? t("connectedHint") : t("disconnectedHint")}
          </p>
          {url ? (
            <>
              <Field id="storefront-shop-url" label={t("url")}>
                <Input value={url} readOnly />
              </Field>
              <div className="flex flex-wrap gap-3">
                <Button
                  variant="outline"
                  disabled={busy}
                  onClick={() => {
                    if (!sameSession() || !url) return
                    if (!navigator.clipboard) {
                      setCopyFailed(true)
                      return
                    }
                    void navigator.clipboard
                      .writeText(url)
                      .then(() => {
                        if (sameSession()) {
                          setCopied(true)
                          setCopyFailed(false)
                        }
                      })
                      .catch(() => {
                        if (sameSession()) setCopyFailed(true)
                      })
                  }}
                >
                  {copied ? t("copied") : t("copy")}
                </Button>
                <a
                  className={buttonVariants({ variant: "outline" })}
                  href={url}
                  target="_blank"
                  rel="noreferrer"
                >
                  {t("open")}
                </a>
              </div>
            </>
          ) : current.active && !busy ? (
            <Notice variant="warning" title={t("addressUnavailable")} />
          ) : null}
          {copyFailed ? (
            <Notice variant="warning" title={t("copyFailed")} />
          ) : null}
          <div>
            <Button
              variant={current.active ? "outline" : "default"}
              disabled={!ready}
              onClick={() => {
                if (!sameSession() || !ready) return
                if (current.active && current.id) setDisconnect(current.id)
                else change(true)
              }}
            >
              {current.active ? t("disconnect") : t("connect")}
            </Button>
          </div>
        </>
      ) : null}
      <ApiError error={update.error} fallback={common("common.error")} />
      <ConfirmDialog
        open={disconnect !== null}
        onOpenChange={(open) => {
          if (!open) setDisconnect(null)
        }}
        title={t("disconnectTitle")}
        description={t("disconnectText")}
        confirm={t("disconnect")}
        cancel={common("actions.cancel")}
        disabled={!ready}
        onConfirm={() => change(false)}
      />
    </Tile>
  )
}
