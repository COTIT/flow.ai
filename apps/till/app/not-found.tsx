import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@/components/state"

export default async function NotFound() {
  const t = await getTranslations("errors")
  return (
    <main className="flex min-h-svh flex-col items-start gap-4 p-6">
      <Notice kind="info">
        <p className="font-medium">{t("notFoundTitle")}</p>
        <p>{t("notFoundBody")}</p>
      </Notice>
      <Link href="/sale" className={buttonVariants()}>
        {t("home")}
      </Link>
    </main>
  )
}
