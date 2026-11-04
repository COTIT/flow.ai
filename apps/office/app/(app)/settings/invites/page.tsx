"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { UserPlus } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import {
  getInvitesListQueryKey,
  useInvitesDelete,
  useInvitesList,
  useInvitesSend,
} from "@genesy/api-client/endpoints/invites"
import { useTeamsList } from "@genesy/api-client/endpoints/teams"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDate } from "@genesy/ui/lib/format"
import { Field, selectClass } from "@/components/field"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Capped, Done, Empty, LoadError, useDone } from "@/components/states"

/** People invited to this account by email (Template List, as OfficeSettings). */
export default function InvitesPage() {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const queryClient = useQueryClient()
  const invites = useInvitesList(
    workspaceId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: getInvitesListQueryKey(workspaceId),
    })
  const done = useDone()
  const withdraw = useInvitesDelete({
    mutation: {
      onMutate: done.clear,
      onSuccess: () => {
        done.show(t("done.inviteWithdrawn"))
        return refresh()
      },
    },
  })
  // The invite being confirmed; kept while the dialog closes so its title does not blank.
  const [withdrawing, setWithdrawing] = React.useState<{
    id: string
    email: string
    open: boolean
  }>()
  const when = (s: string) => formatDate(s, locale)
  const statusLabel = (s: string) =>
    t.has(`invites.statuses.${s}`) ? t(`invites.statuses.${s}`) : s
  const columns = [
    { label: t("invites.email") },
    { label: t("invites.team") },
    { label: t("invites.status") },
    { label: t("invites.sent") },
    { label: t("invites.expires") },
    { label: t("invites.actions"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("invites.title")}
        description={t("invites.intro")}
        actions={
          <SendInvite
            onSent={(email) => {
              done.show(t("done.inviteSent", { email }))
              void refresh()
            }}
            onOpen={done.clear}
          />
        }
      />
      <Done message={done.message} />
      <ApiError error={withdraw.error} fallback={t("common.error")} />
      <ListCard
        pager={
          invites.data?.data.length ? (
            <ListPager meta={invites.data.meta} onPage={setPage} />
          ) : null
        }
      >
        {invites.isPending ? (
          <ListSkeleton columns={columns} />
        ) : invites.isError ? (
          <CardState>
            <LoadError
              error={invites.error}
              onRetry={() => void invites.refetch()}
            />
          </CardState>
        ) : invites.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("invites.empty")} />
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
              {invites.data.data.map((i) => (
                <TableRow key={i.invite_id}>
                  <TableCell className="font-semibold break-all">
                    {i.email}
                  </TableCell>
                  <TableCell data-m="sub">
                    {i.relationships.team?.name ?? "—"}
                  </TableCell>
                  <TableCell data-m="right">
                    <StatusBadge
                      status={i.invite_status}
                      label={statusLabel(i.invite_status)}
                    />
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-muted-foreground tabular-nums"
                  >
                    {when(i.created_at)}
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-muted-foreground tabular-nums"
                  >
                    {when(i.expired_at)}
                  </TableCell>
                  <TableCell data-m="badge" className="text-right">
                    {i.invite_status === "pending" ? (
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={withdraw.isPending}
                        onClick={() =>
                          setWithdrawing({
                            id: i.invite_id,
                            email: i.email,
                            open: true,
                          })
                        }
                      >
                        {t("invites.withdraw")}
                      </Button>
                    ) : null}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
      <ConfirmDialog
        open={withdrawing?.open ?? false}
        onOpenChange={(open) => setWithdrawing((w) => w && { ...w, open })}
        title={t("invites.withdrawTitle", { email: withdrawing?.email ?? "" })}
        description={t("invites.withdrawText")}
        confirm={t("invites.withdraw")}
        cancel={t("actions.cancel")}
        disabled={withdraw.isPending}
        onConfirm={() =>
          withdrawing &&
          withdraw.mutate({ workspace: workspaceId, invite: withdrawing.id })
        }
      />
    </>
  )
}

function SendInvite({
  onSent,
  onOpen,
}: {
  onSent: (email: string) => void
  onOpen: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  // shortcut: first 100 teams in the picker; add search when accounts have more.
  const teams = useTeamsList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: open && Boolean(workspaceId) } }
  )
  const send = useInvitesSend({
    mutation: {
      onSuccess: (_, { data }) => {
        setOpen(false)
        onSent(data.email)
      },
    },
  })
  const errors = fieldErrors(send.error)

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) onOpen()
        else send.reset()
      }}
    >
      <DialogTrigger render={<Button />}>
        <UserPlus aria-hidden="true" strokeWidth={1.75} />
        {t("invites.new")}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            send.mutate({
              workspace: workspaceId,
              data: {
                email: String(f.get("email") ?? "").trim(),
                team_id: String(f.get("team_id") ?? "") || null,
              },
            })
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("invites.new")}</DialogTitle>
            <DialogDescription>{t("invites.newText")}</DialogDescription>
          </DialogHeader>
          <Field id="inv-email" label={t("invites.email")} errors={errors.email}>
            <Input
              name="email"
              type="email"
              autoComplete="off"
              maxLength={255}
              required
            />
          </Field>
          <Capped meta={teams.data?.meta} />
          <div className="flex flex-col gap-1.5">
            <Field
              id="inv-team"
              label={t("invites.team")}
              errors={errors.team_id}
            >
              <select name="team_id" className={selectClass} defaultValue="">
                <option value="">{t("invites.noTeam")}</option>
                {teams.data?.data.map((team) => (
                  <option key={team.id} value={team.id}>
                    {team.name}
                  </option>
                ))}
              </select>
            </Field>
            <p className="text-xs text-muted-foreground">
              {t("invites.teamHint")}
            </p>
          </div>
          <ApiError error={send.error} fallback={t("common.error")} />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              {t("actions.cancel")}
            </DialogClose>
            <Button type="submit" disabled={send.isPending}>
              {t("invites.send")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
