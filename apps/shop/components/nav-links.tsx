"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"

import { cn } from "@genesy/ui/lib/utils"

import { NAV } from "@/lib/nav"

/** The main links; the current section is marked for screen readers and by style. */
export function NavLinks({
  labels,
  className,
  onNavigate,
}: {
  labels: Record<(typeof NAV)[number]["key"], string>
  className?: string
  onNavigate?: () => void
}) {
  const path = usePathname()
  return NAV.map(({ href, key }) => {
    const here =
      path === href ||
      path.startsWith(`${href}/`) ||
      (href === "/shop" && path.startsWith("/products/"))
    return (
      <Link
        key={key}
        href={href}
        onClick={onNavigate}
        aria-current={here ? "page" : undefined}
        className={cn(
          "inline-flex min-h-11 items-center rounded-[12px] px-3.5 font-medium text-muted-foreground transition-colors hover:text-foreground aria-[current=page]:bg-(--glass-subtle) aria-[current=page]:text-foreground md:min-h-[38px]",
          className
        )}
      >
        {labels[key]}
      </Link>
    )
  })
}
