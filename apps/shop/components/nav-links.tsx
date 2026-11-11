"use client"

import Link from "@/components/shop-link"
import { usePathname } from "next/navigation"

import { cn } from "@genesy/ui/lib/utils"

import { NAV } from "@/lib/nav"
import { useShop } from "@/lib/shop-context"

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
  const { base } = useShop()
  const visible = base ? NAV.filter(({ key }) => key === "shop") : NAV
  const localPath =
    base && path.startsWith(base) ? path.slice(base.length) || "/" : path
  return visible.map(({ href, key }) => {
    const here =
      localPath === href ||
      localPath.startsWith(`${href}/`) ||
      (href === "/shop" && localPath.startsWith("/products/"))
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
