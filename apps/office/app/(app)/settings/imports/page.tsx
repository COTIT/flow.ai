"use client"

import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useImportsList } from "@genesy/api-client/endpoints/imports"
import { buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatDateTime } from "@genesy/ui/lib/format"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { LoadError } from "@/components/states"
import { useCan } from "@/lib/can"
import { TONES, type Batch } from "@/lib/imports/flow"

/** Spreadsheet imports of this workspace, newest first; one primary action: start a new one. */
export default function ImportsPage() {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const canCreate = useCan("import.create")
  const list = useImportsList(workspaceId, undefined, {
    query: { enabled: Boolean(workspaceId) },
  })
  const rows = (list.data?.data ?? []) as Batch[]
  const columns = [
    { label: t("imports.file") },
    { label: t("imports.kind") },
    { label: t("imports.state") },
    { label: t("imports.result") },
    { label: t("imports.created") },
  ]
  const kind = (b: Batch) =>
    t(`imports.profileShort.${b.profile}`) +
    (b.mapping.operation === "update"
      ? ` · ${t("imports.profiles.products_update")}`
      : "")

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("imports.title")}
        description={t("imports.intro")}
        actions={
          canCreate ? (
            <Link href="/settings/imports/new" className={buttonVariants()}>
              {t("imports.new")}
            </Link>
          ) : null
        }
      />
      <ListCard>
        {list.isPending ? (
          <ListSkeleton columns={columns} />
        ) : list.isError ? (
          <CardState>
            <LoadError error={list.error} onRetry={() => void list.refetch()} />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <EmptyState
              title={t("imports.empty")}
              description={t("imports.emptyHint")}
            />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((c) => (
                  <TableHead key={c.label}>{c.label}</TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((b) => (
                <TableRow key={b.id}>
                  <TableCell className="font-semibold">
                    <Link
                      href={`/settings/imports/${b.id}`}
                      className="hover:underline"
                    >
                      {b.filename}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub">{kind(b)}</TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={b.state}
                      tone={TONES[b.state]}
                      label={t(`imports.states.${b.state}`)}
                    />
                  </TableCell>
                  <TableCell data-m="sub" className="tabular-nums">
                    {t("imports.counts", b.counts)}
                  </TableCell>
                  <TableCell
                    data-m="hide"
                    className="text-muted-foreground tabular-nums"
                  >
                    {formatDateTime(b.created_at, locale)}
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
