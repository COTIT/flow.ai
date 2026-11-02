import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"

export default async function NotFound() {
  const t = await getTranslations("errors")
  return (
    <main className="flex min-h-svh flex-col items-start gap-4 p-6">
      <Notice variant="info" title={t("notFoundTitle")}>
        {t("notFoundBody")}
      </Notice>
      <Link href="/sale" className={buttonVariants()}>
        {t("home")}
      </Link>
    </main>
  )
}
