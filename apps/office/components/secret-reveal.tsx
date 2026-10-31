"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"

/** A secret the API shows once: copy button and a plain warning. */
export function SecretReveal({ secret }: { secret: string }) {
  const t = useTranslations("secret")
  const [copied, setCopied] = React.useState(false)
  return (
    <div className="flex flex-col gap-2">
      <p className="text-sm font-medium">{t("once")}</p>
      <code
        aria-label={t("label")}
        className="block rounded-lg bg-muted p-2 font-mono text-xs break-all"
      >
        {secret}
      </code>
      <Button
        variant="outline"
        className="self-start"
        onClick={() =>
          navigator.clipboard.writeText(secret).then(
            () => setCopied(true),
            () => setCopied(false)
          )
        }
      >
        {copied ? t("copied") : t("copy")}
      </Button>
    </div>
  )
}

/** "a, b c" → ["a", "b", "c"]: the list inputs for scopes and events. */
export function splitList(value: FormDataEntryValue | null): string[] {
  return String(value ?? "")
    .split(/[\s,]+/)
    .filter(Boolean)
}
