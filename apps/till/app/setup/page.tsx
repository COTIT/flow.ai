"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useTillPair } from "@genesy/api-client/endpoints/till"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Logo } from "@genesy/ui/components/logo"
import { Notice } from "@genesy/ui/components/notice"
import { tillStore } from "@/lib/till-store"

/** Pairs this device with a register: the shop name and the one-time code office shows. */
export default function SetupPage() {
  const t = useTranslations()
  const router = useRouter()
  const pair = useTillPair({
    mutation: {
      onSuccess: ({ data }) => {
        // The key stays in this device's storage; it is never shown.
        tillStore.getState().pair({
          workspaceId: data.workspace_id,
          deviceKey: data.device_key,
        })
        router.replace("/login")
      },
    },
  })
  const status = pair.error?.response?.status
  const problem = !pair.error
    ? null
    : status === 401
      ? { variant: "error" as const, title: t("setup.wrong") }
      : status === 410
        ? { variant: "warning" as const, title: t("setup.used") }
        : status === 429
          ? { variant: "error" as const, title: t("setup.locked") }
          : { variant: "error" as const, title: t("setup.failed") }

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-4">
      <Logo className="h-10" />
      <form
        className="flex w-full max-w-sm flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault()
          const form = new FormData(e.currentTarget)
          pair.mutate({
            data: {
              workspace: String(form.get("workspace")).trim(),
              code: String(form.get("code")).trim(),
            },
          })
        }}
      >
        <h1 className="text-xl font-medium">{t("setup.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("setup.hint")}</p>
        <div className="flex flex-col gap-2">
          <Label htmlFor="setup-workspace">{t("setup.workspace")}</Label>
          <Input
            id="setup-workspace"
            name="workspace"
            autoCapitalize="none"
            autoComplete="off"
            spellCheck={false}
            required
          />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="setup-code">{t("setup.code")}</Label>
          <Input
            id="setup-code"
            name="code"
            className="font-mono tracking-widest uppercase"
            placeholder="K7Q4-MZPX"
            autoCapitalize="characters"
            autoComplete="one-time-code"
            spellCheck={false}
            maxLength={16}
            required
          />
        </div>
        {problem ? <Notice variant={problem.variant} title={problem.title} /> : null}
        {pair.isSuccess ? <Notice variant="success" title={t("setup.paired")} /> : null}
        <Button type="submit" disabled={pair.isPending || pair.isSuccess}>
          {t("setup.submit")}
        </Button>
      </form>
    </main>
  )
}
