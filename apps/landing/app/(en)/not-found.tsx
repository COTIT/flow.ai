import Link from "next/link"
import { CompassIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"

// An unknown path has no language, so the 404 offers all three. It renders in the English root layout.
export default function NotFound() {
  return (
    <main className="grid min-h-svh place-items-center p-4">
      <div className="w-full max-w-lg">
        <EmptyState
          icon={<CompassIcon aria-hidden="true" className="size-8" />}
          title={
            <span role="heading" aria-level={1}>
              This page does not exist
            </span>
          }
          description="The link may be old or mistyped. Pick a language to go to the Genesy home page."
          action={
            <nav
              aria-label="Languages"
              className="flex flex-wrap justify-center gap-2"
            >
              <Link href="/" className={buttonVariants()}>
                English
              </Link>
              <Link
                href="/nl"
                lang="nl"
                className={buttonVariants({ variant: "outline" })}
              >
                Nederlands
              </Link>
              <Link
                href="/sw"
                lang="sw"
                className={buttonVariants({ variant: "outline" })}
              >
                Kiswahili
              </Link>
            </nav>
          }
        />
      </div>
    </main>
  )
}
