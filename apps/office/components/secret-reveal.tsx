"use client"

import * as React from "react"
import { useTranslations } from "next-intl"
import { Copy } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

/**
 * A secret the API shows once (OfficeSettings "Key created"): a warning, the secret in a
 * dashed gold box with a copy button, and "I saved it" to close.
 */
export function SecretReveal({
  secret,
  hint,
  onSaved,
}: {
  secret: string
  /** Where to keep it, under the warning. */
  hint?: string
  onSaved: () => void
}) {
  const t = useTranslations("secret")
  const [copied, setCopied] = React.useState(false)
  return (
    <div className="flex flex-col gap-4">
      <Notice variant="warning" title={t("once")}>
        {hint}
      </Notice>
      <div className="flex items-center gap-2.5 rounded-[14px] border-[1.5px] border-dashed border-gold bg-(--tint-warning) px-3.5 py-3 font-mono text-[13px] break-all">
        <code aria-label={t("label")} className="grow">
          {secret}
        </code>
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            navigator.clipboard.writeText(secret).then(
              () => setCopied(true),
              () => setCopied(false)
            )
          }
        >
          <Copy aria-hidden="true" strokeWidth={1.75} />
          {copied ? t("copied") : t("copy")}
        </Button>
      </div>
      <div className="flex justify-end">
        <Button onClick={onSaved}>{t("saved")}</Button>
      </div>
    </div>
  )
}

/** "a, b c" → ["a", "b", "c"]: the list inputs for scopes and events. */
export function splitList(value: FormDataEntryValue | null): string[] {
  return String(value ?? "")
    .split(/[\s,]+/)
    .filter(Boolean)
}
