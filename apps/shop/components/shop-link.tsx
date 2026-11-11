"use client"

import Link from "next/link"
import { useShop } from "@/lib/shop-context"
import { shopPath } from "@/lib/shop-path"

/** Scoped navigation reloads the shop shell and cart together, including browser back. */
export default function ShopLink(
  props: React.ComponentProps<"a"> & { href: string }
) {
  const { base } = useShop()
  if (!base) return <Link {...props} />
  const { href, ...rest } = props
  return <a {...rest} href={shopPath(base, href)} />
}
