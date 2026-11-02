import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"

/** A scanned code that opens no passport: unknown, mistyped, or not published yet (one 404 for all). */
export default async function BatchNotFound() {
  const t = await getTranslations("errors")
  return (
    <div className="flex flex-col items-start gap-4 py-10">
      <h1 className="heading-1">{t("batchTitle")}</h1>
      <p className="max-w-xl text-muted-foreground">{t("batchBody")}</p>
      <Link href="/" className={buttonVariants()}>
        {t("home")}
      </Link>
    </div>
  )
}
