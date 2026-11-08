"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getUsersListQueryKey,
  useUsersList,
  useUsersMe,
  useUsersRemove,
} from "@genesy/api-client/endpoints/users"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"

/** People with access to this account; an admin ends someone's access (Template List, as Invites). */
export default function MembersPage() {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const myId = useUsersMe().data?.data.user.id
  const [page, setPage] = React.useState(1)
  const queryClient = useQueryClient()
  const members = useUsersList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const done = useDone()
  // The member being confirmed; kept while the dialog closes so its title does not blank.
  const [removing, setRemoving] = React.useState<{
    id: string
    name: string
    open: boolean
  }>()
  const remove = useUsersRemove({
    mutation: {
      onMutate: done.clear,
      onSuccess: (_, { user }) => {
        if (removing?.id === user)
          done.show(t("done.memberRemoved", { name: removing.name }))
        return queryClient.invalidateQueries({
          queryKey: getUsersListQueryKey(workspaceId),
        })
      },
    },
  })
  const columns = [
    { label: t("members.name") },
    { label: t("members.team") },
    { label: t("members.status") },
    { label: t("members.actions"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("members.title")}
        description={t("members.intro")}
      />
      <Done message={done.message} />
      <ApiError error={remove.error} fallback={t("common.error")} />
      <ListCard
        pager={
          members.data?.data.length ? (
            <ListPager meta={members.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {members.isPending ? (
          <ListSkeleton columns={columns} />
        ) : members.isError ? (
          <CardState>
            <LoadError
              error={members.error}
              onRetry={() => void members.refetch()}
            />
          </CardState>
        ) : members.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("members.empty")} />
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
              </TableRow>
            </TableHeader>
            <TableBody>
              {members.data.data.map((m) => {
                const owner = m.permissions[0]?.permissions.includes(
                  "workspace_owner"
                )
                const status = owner ? "owner" : m.active ? "active" : "removed"
                return (
                  <TableRow key={m.id}>
                    <TableCell className="font-semibold">
                      {m.name}
                      <span className="block text-xs font-normal break-all text-muted-foreground">
                        {m.email}
                      </span>
                    </TableCell>
                    <TableCell data-m="sub">
                      {m.teams.map((team) => team.name).join(", ") || "—"}
                    </TableCell>
                    <TableCell data-m="right">
                      <StatusBadge
                        status={status === "removed" ? "revoked" : "active"}
                        label={t(`members.statuses.${status}`)}
                      />
                    </TableCell>
                    <TableCell data-m="badge" className="text-right">
                      {status === "active" && m.id !== myId ? (
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={remove.isPending}
                          onClick={() =>
                            setRemoving({ id: m.id, name: m.name, open: true })
                          }
                        >
                          {t("members.remove")}
                        </Button>
                      ) : null}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}
      </ListCard>
      <ConfirmDialog
        open={removing?.open ?? false}
        onOpenChange={(open) => setRemoving((r) => r && { ...r, open })}
        title={t("members.removeTitle", { name: removing?.name ?? "" })}
        description={t("members.removeText")}
        confirm={t("members.remove")}
        cancel={t("actions.cancel")}
        disabled={remove.isPending}
        onConfirm={() =>
          removing &&
          remove.mutate({ workspace: workspaceId, user: removing.id })
        }
      />
    </>
  )
}
