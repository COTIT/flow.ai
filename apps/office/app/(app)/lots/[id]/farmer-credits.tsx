"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useLotsFarmerCredits } from "@genesy/api-client/endpoints/lots"
import { Notice } from "@genesy/ui/components/notice"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Empty, LoadError } from "@/components/states"

/**
 * Sales and returns of this batch that credited (or debited) its farmers. Groups only:
 * the API never sends farmer names, kg, prices or amounts here (G10).
 */
export function FarmerCredits({ lotId }: { lotId: string }) {
  const t = useTranslations("credits")
  const tc = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const credits = useLotsFarmerCredits(
    workspaceId,
    lotId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )

  return (
    <section className="flex flex-col gap-3">
      <h2 className="heading-2 text-base">{t("title")}</h2>
      {credits.isPending ? (
        <p className="text-sm text-muted-foreground">{tc("common.loading")}</p>
      ) : credits.isError ? (
        <LoadError
          error={credits.error}
          onRetry={() => void credits.refetch()}
        />
      ) : (
        <>
          <Notice
            title={
              credits.data.farmer_groups == null
                ? t("noGroups")
                : t("groups", { count: credits.data.farmer_groups })
            }
          />
          {credits.data.data.length === 0 ? (
            <Empty title={t("empty")} />
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("at")}</TableHead>
                    <TableHead>{t("kind")}</TableHead>
                    <TableHead>{t("order")}</TableHead>
                    <TableHead className="text-right">{t("jars")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {credits.data.data.map((c) => (
                    <TableRow key={`${c.order_id}-${c.kind}-${c.at}`}>
                      <TableCell className="tabular-nums">
                        {new Date(c.at).toLocaleString()}
                      </TableCell>
                      <TableCell>
                        {t.has(`kinds.${c.kind}`)
                          ? t(`kinds.${c.kind}`)
                          : c.kind}
                      </TableCell>
                      <TableCell>
                        <Link
                          className="underline-offset-4 hover:underline"
                          href={`/orders/${c.order_id}`}
                        >
                          {c.order_number}
                        </Link>
                        {c.till_ref ? ` · ${t("till")}` : ""}
                      </TableCell>
                      {/* Units as the API states them; never summed here. */}
                      <TableCell className="text-right tabular-nums">
                        {c.jars}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              <Pager
                page={credits.data.meta.current_page}
                lastPage={credits.data.meta.last_page}
                summary={tc("pager.summary", {
                  from: credits.data.meta.from ?? 0,
                  to: credits.data.meta.to ?? 0,
                  total: credits.data.meta.total,
                })}
                previous={tc("pager.previous")}
                next={tc("pager.next")}
                onPage={setPage}
              />
            </>
          )}
        </>
      )}
    </section>
  )
}
