import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"

export default async function NotFound() {
  const t = await getTranslations("errors")
  return (
    <div className="flex flex-col items-start gap-4 py-10">
      <h1 className="heading-1">{t("notFoundTitle")}</h1>
      <p className="text-muted-foreground">{t("notFoundBody")}</p>
      <Link href="/" className={buttonVariants()}>
        {t("home")}
      </Link>
    </div>
  )
}
