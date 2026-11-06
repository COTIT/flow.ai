"use client"

import { useState } from "react"
import { MenuIcon } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import {
  Sheet,
  SheetContent,
  SheetTitle,
  SheetTrigger,
} from "@genesy/ui/components/sheet"

import { NavLinks } from "./nav-links"

/** Phones: the main links in a side sheet; it closes when a link is followed. */
export function MobileMenu({
  title,
  open: openLabel,
  labels,
  children,
}: {
  title: string
  open: string
  labels: React.ComponentProps<typeof NavLinks>["labels"]
  children?: React.ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger
        render={<Button variant="outline" size="icon" className="md:hidden" />}
      >
        <MenuIcon aria-hidden />
        <span className="sr-only">{openLabel}</span>
      </SheetTrigger>
      <SheetContent side="right" className="gap-2 p-5 pt-14">
        <SheetTitle className="sr-only">{title}</SheetTitle>
        <nav aria-label={title} className="flex flex-col gap-1 text-[16px]">
          <NavLinks labels={labels} onNavigate={() => setOpen(false)} />
        </nav>
        <div className="mt-auto">{children}</div>
      </SheetContent>
    </Sheet>
  )
}
