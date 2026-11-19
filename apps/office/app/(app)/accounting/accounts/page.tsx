"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getAccountingAccountsListQueryKey,
  useAccountingAccountsCreate,
  useAccountingAccountsList,
  useAccountingAccountsUpdate,
} from "@genesy/api-client/endpoints/accounting"
import type { AccountResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Checkbox } from "@genesy/ui/components/checkbox"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Field, selectClass } from "@/components/field"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

import {
  AccountingTabs,
  accountTypes,
  Shell,
  useAction,
  useReasons,
  useWord,
  type Outcome,
} from "../shared"

/** The chart of accounts (Template List): code, name, type, roles; add and edit in dialogs. */
export default function AccountsPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const word = useWord()
  const queryClient = useQueryClient()
  const mayConfigure = useCan("accounting.configure")
  const { page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const accounts = useAccountingAccountsList(
    workspaceId,
    { page: page || undefined, page_size: "100" },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const done = useDone()
  const [stale, setStale] = React.useState(false)
  const outcome: Outcome = {
    done: (message) => {
      setStale(false)
      done.show(message)
      void queryClient.invalidateQueries({
        queryKey: getAccountingAccountsListQueryKey(workspaceId),
      })
    },
    stale: () => {
      done.clear()
      setStale(true)
      void queryClient.invalidateQueries({
        queryKey: getAccountingAccountsListQueryKey(workspaceId),
      })
    },
  }
  const columns = [
    { label: t("accounting.accounts.code") },
    { label: t("accounting.accounts.name") },
    { label: t("accounting.accounts.type") },
    { label: t("accounting.accounts.roles") },
    { label: t("accounting.accounts.state") },
    { label: t("accounting.accounts.actions"), right: true },
  ]
  const rows = accounts.data?.data ?? []

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.accounts.title")}
        tabs={<AccountingTabs />}
        actions={
          mayConfigure ? (
            <AccountDialog
              accounts={rows}
              outcome={outcome}
              button={<Button />}
            />
          ) : null
        }
      />
      <Done message={done.message} />
      {stale ? (
        <Notice
          variant="warning"
          title={t("accounting.reasons.stale_document")}
        />
      ) : null}
      <ListCard
        count={
          accounts.data
            ? t("accounting.accounts.count", {
                total: accounts.data.meta.total,
              })
            : null
        }
        pager={
          accounts.data?.data.length ? (
            <ListPager
              meta={accounts.data.meta}
              onPage={(p) => router.push(`?page=${p}`)}
            />
          ) : null
        }
      >
        {accounts.isPending ? (
          <ListSkeleton columns={columns} />
        ) : accounts.isError ? (
          <CardState>
            <LoadError
              error={accounts.error}
              onRetry={() => void accounts.refetch()}
            />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <Empty title={t("accounting.accounts.empty")} />
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
              {rows.map((a) => (
                <TableRow key={a.id}>
                  <TableCell className="font-semibold tabular-nums">
                    <Link
                      className="underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
                      href={`/accounting/accounts/${a.id}`}
                    >
                      {a.code}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub">{a.name}</TableCell>
                  <TableCell data-m="hide">{word("types", a.type)}</TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-[13px] text-muted-foreground"
                  >
                    {a.roles.map((r) => word("roles", r)).join(", ")}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={a.active ? "active" : "inactive"}
                      label={t(
                        a.active
                          ? "accounting.accounts.active"
                          : "accounting.accounts.inactive"
                      )}
                      tone={a.active ? "teal" : "slate"}
                    />
                  </TableCell>
                  <TableCell data-m="right" className="text-right">
                    {mayConfigure && a.allowed_actions.includes("update") ? (
                      <EditAccountDialog
                        account={a}
                        outcome={outcome}
                        button={<Button variant="outline" size="sm" />}
                      />
                    ) : null}
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

/** Add an account: code, name, type, an optional parent of the same type. */
function AccountDialog({
  accounts,
  outcome,
  button,
}: {
  accounts: AccountResource[]
  outcome: Outcome
  button: React.ReactElement
}) {
  const t = useTranslations()
  const word = useWord()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [type, setType] = React.useState<string>("asset")
  const create = useAccountingAccountsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        outcome.done(t("done.accounting.accountCreated", { code: r.data.code }))
      },
    },
  })
  const errors = fieldErrors(create.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        create.reset()
      }}
      button={button}
      trigger={t("accounting.accounts.new")}
      title={t("accounting.accounts.new")}
      description={t("accounting.accounts.newText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          const text = (k: string) => String(f.get(k) ?? "").trim()
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: {
                code: text("code"),
                name: text("name"),
                type: text("type"),
                parent_id: text("parent_id") || null,
              },
            },
          })
        }}
      >
        <Field
          id="acc-code"
          label={t("accounting.accounts.code")}
          errors={errors["payload.code"]}
        >
          <Input name="code" required maxLength={32} />
        </Field>
        <Field
          id="acc-name"
          label={t("accounting.accounts.name")}
          errors={errors["payload.name"]}
        >
          <Input name="name" required maxLength={255} />
        </Field>
        <Field
          id="acc-type"
          label={t("accounting.accounts.type")}
          errors={errors["payload.type"]}
        >
          <select
            name="type"
            className={selectClass}
            value={type}
            onChange={(e) => setType(e.target.value)}
          >
            {accountTypes.map((x) => (
              <option key={x} value={x}>
                {word("types", x)}
              </option>
            ))}
          </select>
        </Field>
        <Field
          id="acc-parent"
          label={t("accounting.accounts.parent")}
          errors={errors["payload.parent_id"]}
        >
          <select name="parent_id" className={selectClass} defaultValue="">
            <option value="">{t("accounting.accounts.noParent")}</option>
            {accounts
              .filter((x) => x.type === type && x.active)
              .map((x) => (
                <option key={x.id} value={x.id}>
                  {x.code} {x.name}
                </option>
              ))}
          </select>
        </Field>
        <ApiError
          error={create.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={create.isPending}>
            {t("accounting.accounts.create")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Rename an account, or switch it off (the API refuses one mapped to a role). */
function EditAccountDialog({
  account,
  outcome,
  button,
}: {
  account: AccountResource
  outcome: Outcome
  button: React.ReactElement
}) {
  const t = useTranslations()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const update = useAccountingAccountsUpdate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        outcome.done(t("done.accounting.accountSaved", { code: r.data.code }))
      },
    },
  })
  const errors = fieldErrors(update.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        update.reset()
      }}
      button={button}
      trigger={t("accounting.accounts.edit")}
      title={t("accounting.accounts.editTitle", { code: account.code })}
      description={t("accounting.accounts.editText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          update.mutate({
            workspace: workspaceId,
            account: account.id,
            data: {
              client_ref: a.clientRef,
              expected_version: account.version,
              payload: {
                name: String(f.get("name") ?? "").trim(),
                active: f.get("active") === "on",
              },
            },
          })
        }}
      >
        <Field
          id="acc-edit-name"
          label={t("accounting.accounts.name")}
          errors={errors["payload.name"]}
        >
          <Input
            name="name"
            required
            maxLength={255}
            defaultValue={account.name}
          />
        </Field>
        <label className="flex items-center gap-2 text-sm max-md:min-h-11">
          <Checkbox name="active" value="on" defaultChecked={account.active} />
          {t("accounting.accounts.inUse")}
        </label>
        {errors["payload.active"]?.length ? (
          <p className="text-sm text-destructive">
            {errors["payload.active"].join(" ")}
          </p>
        ) : null}
        <ApiError
          error={update.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={update.isPending}>
            {t("actions.save")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}
