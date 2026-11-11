"use client"

import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { useVoice } from "@genesy/voice/components/provider"
import { useOnceIntent } from "@/lib/use-once-intent"

/**
 * A review that expired while open: said where the card was (the dock says it too), with one
 * way forward. Nothing was sent; asking again gets a fresh proposal to review.
 */
export function Expired() {
  const t = useTranslations("task.expired")
  const voice = useVoice()
  const said = useTranslations("voice")("expired")
  const ask = useOnceIntent("expired")
  // The provider marks an expired review only by this error text (no flag): the same message key.
  if (voice.review || voice.error !== said) return null
  return (
    <Notice
      variant="warning"
      title={t("title")}
      action={
        voice.connected ? (
          <Button
            variant="outline"
            className="min-h-11"
            disabled={ask.pending}
            aria-busy={ask.pending}
            onClick={() => void ask.send(t("askAgainMessage"))}
          >
            {t("askAgain")}
          </Button>
        ) : null
      }
    >
      {said}
    </Notice>
  )
}
