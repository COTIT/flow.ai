"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getSupplyLinksListQueryKey,
  useSupplyLinksAccept,
  useSupplyLinksList,
  useSupplyLinksRevoke,
} from "@genesy/api-client/endpoints/supply-links"
import { ApiError } from "@genesy/ui/components/api-error"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

export default function SupplyLinksPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const [page, setPage] = React.useState(1)
  const links = useSupplyLinksList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const onSuccess = () =>
    queryClient.invalidateQueries({
      queryKey: getSupplyLinksListQueryKey(workspaceId),
    })
  const accept = useSupplyLinksAccept({ mutation: { onSuccess } })
  const revoke = useSupplyLinksRevoke({ mutation: { onSuccess } })
  const label = (group: string, key: string) =>
    t.has(`supply.${group}.${key}`) ? t(`supply.${group}.${key}`) : key
  // The allow-list in words; an empty list still lets the lot cross, with no farmer details.
  const allowList = (fields: string[]) =>
    fields.length
      ? fields.map((f) => label("fields", f)).join(", ")
      : t("supply.noFields")
  const day = (s: string | null) => s?.slice(0, 10) ?? "—"

  if (links.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (links.isError)
    return (
      <LoadError error={links.error} onRetry={() => void links.refetch()} />
    )
  return (
    <>
      <p className="text-sm text-muted-foreground">{t("supply.linksHint")}</p>
      <ApiError
        error={accept.error ?? revoke.error}
        fallback={t("common.error")}
      />
      {links.data.data.length === 0 ? (
        <Empty title={t("supply.noLinks")} />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("supply.partner")}</TableHead>
                <TableHead>{t("supply.direction")}</TableHead>
                <TableHead>{t("lots.status")}</TableHead>
                <TableHead>{t("supply.allowList")}</TableHead>
                <TableHead>{t("supply.granted")}</TableHead>
                <TableHead>
                  <span className="sr-only">{t("appKeys.actions")}</span>
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {links.data.data.map((l) => (
                <TableRow key={l.id}>
                  <TableCell className="font-medium">
                    {l.partner.name}
                  </TableCell>
                  <TableCell>{label("directions", l.direction)}</TableCell>
                  <TableCell>
                    <StatusBadge
                      status={
                        l.status === "live"
                          ? "active"
                          : l.status === "revoked"
                            ? "revoked"
                            : l.status
                      }
                      label={label("statuses", l.status)}
                    />
                  </TableCell>
                  <TableCell className="whitespace-normal">
                    {allowList(l.fields)}
                  </TableCell>
                  <TableCell className="tabular-nums">
                    {day(l.granted_at)}
                  </TableCell>
                  <TableCell>
                    <div className="flex justify-end gap-2">
                      {l.direction === "incoming" && l.status === "pending" ? (
                        <ConfirmDialog
                          trigger={t("supply.accept")}
                          destructive={false}
                          title={t("supply.acceptTitle", {
                            partner: l.partner.name,
                          })}
                          description={t("supply.acceptText", {
                            fields: allowList(l.fields),
                          })}
                          confirm={t("supply.accept")}
                          cancel={t("actions.cancel")}
                          disabled={accept.isPending}
                          onConfirm={() =>
                            accept.mutate({
                              workspace: workspaceId,
                              link: l.id,
                            })
                          }
                        />
                      ) : null}
                      {l.status !== "revoked" ? (
                        <ConfirmDialog
                          trigger={t("supply.revoke")}
                          title={t("supply.revokeTitle", {
                            partner: l.partner.name,
                          })}
                          description={t("supply.revokeText")}
                          confirm={t("supply.revoke")}
                          cancel={t("actions.cancel")}
                          disabled={revoke.isPending}
                          onConfirm={() =>
                            revoke.mutate({
                              workspace: workspaceId,
                              link: l.id,
                            })
                          }
                        />
                      ) : null}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={links.data.meta.current_page}
            lastPage={links.data.meta.last_page}
            summary={t("pager.summary", {
              from: links.data.meta.from ?? 0,
              to: links.data.meta.to ?? 0,
              total: links.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={setPage}
          />
        </>
      )}
    </>
  )
}
