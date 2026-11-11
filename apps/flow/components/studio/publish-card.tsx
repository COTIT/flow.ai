"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"

/**
 * Explicit approval of one publication: the live and the proposed revision side by side. Confirm
 * goes through the shared review's confirm(), which re-checks revision, version, login and
 * workspace before the one write; a 409 closes this card with the conflict notice.
 */
export function PublishCard({
  live,
  proposed,
  name,
  busy,
  ready,
  onConfirm,
  onCancel,
}: {
  live: string
  proposed: string
  name: string
  busy: boolean
  ready: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const t = useTranslations("studio")
  return (
    <section
      aria-label={t("publishTitle")}
      className="flex flex-col gap-3 rounded-2xl border border-gold bg-card p-4"
    >
      <p className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">
        {t("publishTitle")}
      </p>
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
        <dt className="text-muted-foreground">{t("currentLive")}</dt>
        <dd>{live}</dd>
        <dt className="text-muted-foreground">{t("proposed")}</dt>
        <dd>{proposed}</dd>
        <dt className="text-muted-foreground">{t("target")}</dt>
        <dd>{t("targetValue", { name })}</dd>
      </dl>
      <p className="text-sm">{t("replace")}</p>
      <p role="status" className="text-sm text-muted-foreground">
        {busy ? t("publishing") : t("waiting")}
      </p>
      <div className="flex flex-wrap gap-2">
        <Button className="min-h-11" disabled={busy || !ready} onClick={onConfirm}>
          {t("confirm")}
        </Button>
        <Button
          variant="outline"
          className="min-h-11"
          disabled={busy}
          onClick={onCancel}
        >
          {t("cancel")}
        </Button>
      </div>
    </section>
  )
}
