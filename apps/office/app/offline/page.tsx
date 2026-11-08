import { WifiOffIcon } from "lucide-react"
import { getTranslations } from "next-intl/server"

import { Card, CardContent } from "@genesy/ui/components/card"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Retry } from "./retry"

/** Shown by the service worker for a page it doesn't have when there is no connection. */
export default async function Offline() {
  const t = await getTranslations("states")
  return (
    <main className="flex min-h-svh flex-col justify-center p-4">
      <Card className="mx-auto w-full max-w-lg">
        <CardContent>
          <EmptyState
            icon={<WifiOffIcon aria-hidden="true" className="size-8" />}
            title={t("offline")}
            description={t("offlineText")}
            action={<Retry>{t("retry")}</Retry>}
          />
        </CardContent>
      </Card>
    </main>
  )
}
