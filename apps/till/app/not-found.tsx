import Link from "next/link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { cn } from "@genesy/ui/lib/utils"

/** An address the till doesn't have (POS panel). */
export default async function NotFound() {
  const t = await getTranslations("errors")
  return (
    <main className="till">
      <div className="till-center">
        <section className="till-card strong w-[min(620px,100%)] gap-[18px] p-7 max-[760px]:p-4">
          <Notice variant="info" title={t("notFoundTitle")}>
            {t("notFoundBody")}
          </Notice>
          <Link
            href="/sale"
            className={cn(buttonVariants(), "btn-64 w-full text-base")}
          >
            {t("home")}
          </Link>
        </section>
      </div>
    </main>
  )
}
