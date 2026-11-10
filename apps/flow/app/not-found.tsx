import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { Card, CardContent } from "@genesy/ui/components/card"
import { EmptyState } from "@genesy/ui/components/empty-state"

/** An address Flow does not answer. */
export default async function NotFound() {
  const t = await getTranslations("flow")
  return (
    <main className="flex min-h-svh flex-col justify-center p-4">
      <Card className="mx-auto w-full max-w-lg">
        <CardContent>
          <EmptyState
            title={<span role="heading" aria-level={1}>{t("pageNotFound")}</span>}
            action={
              <Link
                className="text-sm font-medium underline-offset-4 hover:underline"
                href="/"
              >
                {t("goHome")}
              </Link>
            }
          />
        </CardContent>
      </Card>
    </main>
  )
}
