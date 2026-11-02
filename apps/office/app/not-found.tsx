import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { EmptyState } from "@genesy/ui/components/empty-state"

/** An address no office screen answers. */
export default async function NotFound() {
  const t = await getTranslations("states")
  return (
    <main className="mx-auto flex min-h-svh max-w-lg flex-col justify-center p-4">
      <EmptyState
        title={t("pageNotFound")}
        action={
          <Link
            className="text-sm font-medium underline-offset-4 hover:underline"
            href="/products"
          >
            {t("goHome")}
          </Link>
        }
      />
    </main>
  )
}
