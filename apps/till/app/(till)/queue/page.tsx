"use client"

import { useTranslations } from "next-intl"

import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { estimate } from "@/lib/cart-store"
import { queueStore, replay, useQueue } from "@/lib/sale-queue"
import { useTillMoney } from "@/lib/use-money"

/** Sales rung up offline: waiting, booked by the server, or refused and needing a look. */
export default function QueuePage() {
  const t = useTranslations("queue")
  const { money } = useTillMoney()
  const items = useQueue((s) => s.items)
  const syncing = useQueue((s) => s.syncing)

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
      {items.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("empty")}</p>
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
                  ? t("booked", { number: i.sale.number, due: money(i.sale.due) })
                  : `≈ ${money(estimate(i.lines))}`}
                {i.message ? (
                  <span className="block text-destructive">{i.message}</span>
                ) : null}
              </span>
              {i.status !== "pending" ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => queueStore.getState().dismiss(i.body.client_ref)}
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
