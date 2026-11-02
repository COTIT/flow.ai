"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { Calculator } from "lucide-react"

import { useTillPair } from "@genesy/api-client/endpoints/till"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Notice } from "@genesy/ui/components/notice"
import { AuthTop } from "@/components/auth-top"
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
          registerName: data.register.name,
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
    <div className="till">
      <AuthTop />
      <main className="till-center">
        <form
          className="till-card strong w-[min(560px,100%)] gap-[18px] p-8 max-[760px]:p-4"
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
          <span className="chip-ic gold xl">
            <Calculator aria-hidden className="size-7" />
          </span>
          <div>
            <h1 className="till-h2">{t("setup.title")}</h1>
            <p className="text-muted-foreground">{t("setup.hint")}</p>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="setup-workspace">{t("setup.workspace")}</Label>
            <Input
              id="setup-workspace"
              name="workspace"
              className="h-14 rounded-xl px-3.5 text-base md:text-base"
              autoCapitalize="none"
              autoComplete="off"
              spellCheck={false}
              required
            />
          </div>
          {problem ? (
            <Notice variant={problem.variant} title={problem.title} />
          ) : null}
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="setup-code">{t("setup.code")}</Label>
            <Input
              id="setup-code"
              name="code"
              className="h-16 rounded-xl px-3.5 text-center font-mono text-[26px] tracking-[0.2em] uppercase md:text-[26px]"
              placeholder="K7Q4-MZPX"
              autoCapitalize="characters"
              autoComplete="one-time-code"
              spellCheck={false}
              maxLength={16}
              required
            />
            <p className="text-xs text-muted-foreground">
              {t("setup.codeHint")}
            </p>
          </div>
          {pair.isSuccess ? (
            <Notice variant="success" title={t("setup.paired")} />
          ) : null}
          <Button
            type="submit"
            className="btn-64 w-full"
            disabled={pair.isPending || pair.isSuccess}
          >
            {t("setup.submit")}
          </Button>
        </form>
      </main>
    </div>
  )
}
