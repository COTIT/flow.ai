import Link from "next/link"

import { cn } from "@genesy/ui/lib/utils"

import type { StorefrontCategory } from "@/lib/storefront"

const chip =
  "inline-flex min-h-11 shrink-0 items-center rounded-full border border-(--hair) bg-(--glass-subtle) px-4 font-medium whitespace-nowrap hover:bg-(--glass-strong) aria-[current=page]:border-transparent aria-[current=page]:bg-primary aria-[current=page]:text-primary-foreground md:min-h-10"

/** Category filter as links (a scrolling row on phones); the chosen one is the current page. */
export function CategoryChips({
  categories,
  current,
  label,
  all,
}: {
  categories: StorefrontCategory[]
  current?: string
  label: string
  all: string
}) {
  return (
    <nav
      aria-label={label}
      className="-mx-3 overflow-x-auto px-3 md:mx-0 md:px-0"
    >
      <ul className="flex gap-2 pb-1 md:flex-wrap">
        <li>
          <Link
            href="/shop"
            aria-current={current ? undefined : "page"}
            className={chip}
          >
            {all}
          </Link>
        </li>
        {categories.map((c) => (
          <li key={c.id}>
            <Link
              href={`/shop?category=${c.id}`}
              aria-current={current === c.id ? "page" : undefined}
              className={cn(chip)}
            >
              {c.name}
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  )
}
