"use client"

import { useTranslations } from "next-intl"

import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { estimate } from "@/lib/cart-store"
import { queueStore, replay, useQueue } from "@/lib/sale-queue"
import { useTillMoney } from "@/lib/use-money"

/** Sales rung up offline: waiting, booked by the server, or refused and needing a look. */
export default function QueuePage() {
  const t = useTranslations("queue")
  const { money } = useTillMoney()
  const items = useQueue((s) => s.items)
  const syncing = useQueue((s) => s.syncing)
  const allBooked = items.length > 0 && items.every((i) => i.status === "done")

  return (
    <section className="flex max-w-xl flex-col gap-4">
      <h1 className="text-xl font-medium">{t("title")}</h1>
      <Button
        className="w-fit"
        disabled={syncing}
        onClick={() => void replay()}
      >
        {t("sync")}
      </Button>
      {syncing ? <Notice variant="info" title={t("syncing")} /> : null}
      {allBooked ? <Notice variant="success" title={t("allBooked")} /> : null}
      {items.length === 0 ? (
        <EmptyState title={t("empty")} description={t("emptyHint")} />
      ) : (
        <ul className="flex flex-col gap-2">
          {items.map((i) => (
            <li
              key={i.body.client_ref}
              className="flex flex-wrap items-center gap-2 rounded-lg border p-3 text-sm"
            >
              <Badge
                variant={
                  i.status === "done"
                    ? "secondary"
                    : i.status === "pending"
                      ? "outline"
                      : "destructive"
                }
              >
                {t(i.status)}
              </Badge>
              <span className="mr-auto">
                {i.sale
                  ? t("booked", {
                      number: i.sale.number,
                      due: money(i.sale.due),
                    })
                  : `≈ ${money(estimate(i.lines))}`}
              </span>
              {i.status === "conflict" || i.status === "review" ? (
                <Notice
                  variant="error"
                  className="w-full"
                  title={i.message ?? t(i.status)}
                >
                  {t("showOffice")}
                </Notice>
              ) : null}
              {i.sale?.price_mismatch ? (
                <Notice
                  variant="warning"
                  className="w-full"
                  title={t("mismatch")}
                />
              ) : null}
              {i.status !== "pending" ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() =>
                    queueStore.getState().dismiss(i.body.client_ref)
                  }
                >
                  {t("dismiss")}
                </Button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
