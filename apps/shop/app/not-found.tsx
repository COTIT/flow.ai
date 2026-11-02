import { getTranslations } from "next-intl/server"

import { NotFoundCard } from "@/components/not-found-card"

/** Template States: the empty state in a card, back to the shop. */
export default async function NotFound() {
  const t = await getTranslations("errors")
  return (
    <NotFoundCard
      title={t("notFoundTitle")}
      description={t("notFoundBody")}
      action={t("home")}
    />
  )
}
