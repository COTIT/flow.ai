import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"

/** Shown by the service worker for an address it doesn't have, with no connection (POS panel). */
export default async function Offline() {
  const t = await getTranslations()
  return (
    <main className="till">
      <div className="till-center">
        <section className="till-card strong w-[min(620px,100%)] gap-[18px] p-7 max-[760px]:p-4">
          <Notice variant="warning" title={t("offline.title")}>
            {t("offline.body")}
          </Notice>
          <Link
            href="/sale"
            className={cn(buttonVariants(), "btn-64 w-full text-base")}
          >
            {t("errors.home")}
          </Link>
        </section>
      </div>
    </main>
  )
}
