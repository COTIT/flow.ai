import Link from "next/link"

import { buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"

import { card } from "@/lib/look"

/** Mock `ShopStates` "Passport not found": the empty state in a card, its title the page's h1. */
export function NotFoundCard({
  icon,
  title,
  description,
  action,
}: {
  icon?: React.ReactNode
  title: string
  description: string
  action: string
}) {
  return (
    <div className={card}>
      <EmptyState
        icon={icon}
        // EmptyState draws its title in a <p>; a span keeps the markup valid.
        title={
          <span role="heading" aria-level={1}>
            {title}
          </span>
        }
        description={description}
        action={
          <Link
            href="/"
            data-slot="button"
            className={buttonVariants({ variant: "outline", size: "sm" })}
          >
            {action}
          </Link>
        }
      />
    </div>
  )
}
