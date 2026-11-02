import { getTranslations } from "next-intl/server"
import { QrCodeIcon } from "lucide-react"

import { NotFoundCard } from "@/components/not-found-card"

/** A scanned code that opens no passport: unknown, mistyped, or not published yet (one 404 for all). */
export default async function BatchNotFound() {
  const t = await getTranslations("errors")
  return (
    <NotFoundCard
      icon={<QrCodeIcon aria-hidden="true" className="size-[22px]" />}
      title={t("batchTitle")}
      description={t("batchBody")}
      action={t("home")}
    />
  )
}
