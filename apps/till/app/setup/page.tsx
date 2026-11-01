"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Logo } from "@genesy/ui/components/logo"
import { tillStore } from "@/lib/till-store"

/** Pairs this device with a register: the workspace id and the device key office showed once. */
export default function SetupPage() {
  const t = useTranslations()
  const router = useRouter()

  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-6 p-4">
      <Logo className="h-10" />
      <form
        className="flex w-full max-w-sm flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault()
          const form = new FormData(e.currentTarget)
          tillStore.getState().pair({
            workspaceId: String(form.get("workspace")).trim(),
            deviceKey: String(form.get("key")).trim(),
          })
          router.replace("/login")
        }}
      >
        <h1 className="text-xl font-medium">{t("setup.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("setup.hint")}</p>
        <div className="flex flex-col gap-2">
          <Label htmlFor="setup-workspace">{t("setup.workspace")}</Label>
          <Input id="setup-workspace" name="workspace" required />
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="setup-key">{t("setup.key")}</Label>
          <Input
            id="setup-key"
            name="key"
            type="password"
            autoComplete="off"
            required
          />
        </div>
        <Button type="submit">{t("setup.submit")}</Button>
      </form>
    </main>
  )
}
