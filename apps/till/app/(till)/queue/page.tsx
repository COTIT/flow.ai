"use client"

import { useLocale, useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"
import { estimate } from "@/lib/cart-store"
import { queueStore, replay, useQueue } from "@/lib/sale-queue"
import { useOnline } from "@/lib/use-online"
import { useTillMoney } from "@/lib/use-money"
import { refusal } from "@/lib/refusal"

const tone = {
  pending: "gold",
  done: "teal",
  conflict: "red",
  review: "red",
} as const

/** Sales rung up offline (TillQueue): waiting, booked by the server, or refused and needing a look. */
export default function QueuePage() {
  const t = useTranslations()
  const locale = useLocale()
  const { money } = useTillMoney()
  const online = useOnline()
  const items = useQueue((s) => s.items)
  const syncing = useQueue((s) => s.syncing)
  const allBooked = items.length > 0 && items.every((i) => i.status === "done")
  const waiting = items.filter((i) => i.status === "pending").length
  const review = items.filter(
    (i) => i.status === "conflict" || i.status === "review"
  ).length
  const sync = (
    <Button
      variant={online ? "outline" : "default"}
      className="h-12"
      disabled={syncing}
      onClick={() => void replay()}
    >
      {t("queue.sync")}
    </Button>
  )

  return (
    <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
      {online ? null : (
        <Notice variant="warning" title={t("offline.title")} action={sync}>
          {t("offline.body")}
        </Notice>
      )}
      <section className="till-card gap-2.5" aria-labelledby="queue-title">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h1 id="queue-title" className="card-title text-lg">
            {t("queue.title")}
          </h1>
          <div className="flex items-center gap-3">
            {items.length ? (
              <span className="text-[13px] text-muted-foreground">
                {[
                  waiting ? t("queue.waiting", { count: waiting }) : null,
                  review ? t("queue.needsOffice", { count: review }) : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </span>
            ) : null}
            {online ? sync : null}
          </div>
        </div>
        {/* Replay also runs every 30 s; say so only while something is waiting. */}
        {syncing && waiting ? (
          <Notice variant="info" title={t("queue.syncing")} />
        ) : null}
        {allBooked ? (
          <Notice variant="success" title={t("queue.allBooked")} />
        ) : null}
        {items.length === 0 ? (
          <EmptyState
            title={t("queue.empty")}
            description={t("queue.emptyHint")}
          />
        ) : (
          <ul className="flex flex-col gap-2.5">
            {items.map((i) => {
              const pieces = i.lines.reduce((n, l) => n + l.quantity, 0)
              const refused = i.status === "conflict" || i.status === "review"
              const said = refusal(
                i.message,
                (k) => t(`refusals.${k}`),
                // Not the status word: the card's badge already says it.
                t("sale.refused")
              )
              return (
                <li
                  key={i.body.client_ref}
                  className={cn(
                    "till-tile flex flex-col gap-2",
                    refused && "border-[var(--c-red)]"
                  )}
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <p className="font-semibold">
                        {i.body.sold_at
                          ? `${new Intl.DateTimeFormat(locale, {
                              timeStyle: "short",
                            }).format(new Date(i.body.sold_at))} · `
                          : null}
                        {t("queue.items", { count: pieces })}
                      </p>
                      <p className="text-[13px] text-muted-foreground">
                        {i.sale
                          ? t("queue.booked", {
                              number: i.sale.number,
                              due: money(i.sale.due),
                            })
                          : t("queue.estimate", {
                              method:
                                i.body.payment.method === "cash"
                                  ? t("sale.cash")
                                  : [t("sale.card"), i.body.payment.reference]
                                      .filter(Boolean)
                                      .join(" "),
                              amount: money(estimate(i.lines)),
                            })}
                      </p>
                    </div>
                    <span className={`tbadge ${tone[i.status]}`}>
                      {t(`queue.${i.status}`)}
                    </span>
                  </div>
                  {refused ? (
                    <Notice
                      variant="error"
                      className="px-3.5 py-2.5"
                      title={said.title}
                    >
                      {said.detail ? <p>{said.detail}</p> : null}
                      <p>{t("queue.showOffice")}</p>
                    </Notice>
                  ) : null}
                  {i.sale?.unsellable_lines ? (
                    <Notice
                      variant="warning"
                      className="px-3.5 py-2.5"
                      title={t("queue.unsellable")}
                    />
                  ) : null}
                  {i.sale?.price_mismatch ? (
                    <Notice
                      variant="warning"
                      className="px-3.5 py-2.5"
                      title={t("queue.mismatch")}
                    />
                  ) : null}
                  {i.status !== "pending" ? (
                    <div className="flex justify-end">
                      <Button
                        variant="outline"
                        className="h-12"
                        onClick={() =>
                          queueStore.getState().dismiss(i.body.client_ref)
                        }
                      >
                        {t("queue.dismiss")}
                      </Button>
                    </div>
                  ) : null}
                </li>
              )
            })}
          </ul>
        )}
      </section>
    </main>
  )
}
