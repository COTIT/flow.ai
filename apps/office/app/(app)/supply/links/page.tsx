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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { CardState, ListCard, ListPager, ListSkeleton } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"

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
  const done = useDone()
  const ok = (message: string) => ({
    onMutate: done.clear,
    onSuccess: () => {
      done.show(message)
      return onSuccess()
    },
  })
  const accept = useSupplyLinksAccept({ mutation: ok(t("done.linkAccepted")) })
  const revoke = useSupplyLinksRevoke({ mutation: ok(t("done.linkRevoked")) })
  const label = (group: string, key: string) =>
    t.has(`supply.${group}.${key}`) ? t(`supply.${group}.${key}`) : key
  // The allow-list in words; an empty list still lets the lot cross, with no farmer details.
  const allowList = (fields: string[]) =>
    fields.length
      ? fields.map((f) => label("fields", f)).join(", ")
      : t("supply.noFields")
  const day = (s: string | null) => s?.slice(0, 10) ?? "—"

  const columns = [
    { label: t("supply.partner") },
    { label: t("supply.direction") },
    { label: t("lots.status") },
    { label: t("supply.allowList") },
    { label: t("supply.granted") },
  ]

  return (
    <>
      <p className="text-sm text-muted-foreground">{t("supply.linksHint")}</p>
      <Done message={done.message} />
      <ApiError
        error={accept.error ?? revoke.error}
        fallback={t("common.error")}
      />
      <ListCard
        count={
          links.data
            ? t("supply.linksCount", { total: links.data.meta.total })
            : null
        }
        pager={
          links.data?.data.length ? (
            <ListPager meta={links.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {links.isPending ? (
          <ListSkeleton columns={columns} />
        ) : links.isError ? (
          <CardState>
            <LoadError
              error={links.error}
              onRetry={() => void links.refetch()}
            />
          </CardState>
        ) : links.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("supply.noLinks")} />
          </CardState>
        ) : (
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
                  <TableCell className="font-semibold">
                    {l.partner.name}
                  </TableCell>
                  <TableCell data-m="sub">
                    {label("directions", l.direction)}
                  </TableCell>
                  <TableCell data-m="right">
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
                  <TableCell
                    data-m="sub"
                    className="whitespace-normal text-muted-foreground"
                  >
                    {allowList(l.fields)}
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-muted-foreground tabular-nums"
                  >
                    {day(l.granted_at)}
                  </TableCell>
                  <TableCell data-m="badge">
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
        )}
      </ListCard>
    </>
  )
}
