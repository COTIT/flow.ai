"use client"

import * as React from "react"
import Link from "next/link"
import type { LucideIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { cn } from "@genesy/ui/lib/utils"

// Bento pieces from the detail mockups (OfficeOverview, OfficeProduct, OfficeLot, OfficeOrder).

/** `.bento`: 12 columns, 16 px gaps; every child takes the full width on a phone. */
export function Bento({
  className,
  ...props
}: React.ComponentProps<"section">) {
  return (
    <section
      {...props}
      className={cn(
        "grid grid-cols-12 gap-4 max-md:gap-3 max-md:*:col-span-12",
        className
      )}
    ></section>
  )
}

export type Tone = "teal" | "gold" | "red" | "slate"

// glass.css `.chip-ic`: a 36 px tinted square holding a tone-coloured icon.
export const chipTone: Record<Tone, string> = {
  teal: "bg-(--tint-success) text-(--tone-success)",
  gold: "bg-(--tint-warning) text-(--tone-warning)",
  red: "bg-(--tint-error) text-(--tone-error)",
  slate: "bg-(--tint-info) text-(--tone-info)",
}

export function Chip({ icon: Icon, tone }: { icon: LucideIcon; tone: Tone }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex size-9 shrink-0 items-center justify-center rounded-xl",
        chipTone[tone]
      )}
    >
      <Icon className="size-[18px]" strokeWidth={1.75} />
    </span>
  )
}

/** `.tile`: a tinted block inside a card, never blurred. */
export function SubTile({
  children,
  className,
}: {
  children: React.ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        "rounded-2xl border border-(--hair) bg-(--glass-subtle) p-4",
        className
      )}
    >
      {children}
    </div>
  )
}

/** A small muted label over a 22 px number ("On hand 214"), in a SubTile. */
export function Metric({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <SubTile>
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="text-[22px] leading-[1.3] font-medium tabular-nums">
        {children}
      </p>
    </SubTile>
  )
}

/** OfficeStates "Loading · bento detail": tile-shaped blocks while a detail page loads. */
export function DetailSkeleton() {
  return (
    <Bento aria-busy="true">
      <Skeleton className="col-span-8 h-[320px] rounded-[20px]" />
      <Skeleton className="col-span-4 h-[320px] rounded-[20px]" />
      <Skeleton className="col-span-12 h-[220px] rounded-[20px]" />
    </Bento>
  )
}

/** A bento card with a labelled title row (`.card-head`). */
export function Tile({
  id,
  title,
  sub,
  action,
  span,
  flush = false,
  children,
}: {
  id: string
  title: string
  sub?: string
  action?: React.ReactNode
  span: string
  flush?: boolean
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className={cn(
        span,
        "max-md:col-span-12",
        flush ? "gap-0 py-0" : "gap-4 px-6 py-6 max-md:gap-3 max-md:p-4"
      )}
    >
      <div
        className={cn(
          "flex flex-wrap items-center justify-between gap-3",
          flush && "px-6 pt-5 pb-4 max-md:px-4 max-md:pt-4 max-md:pb-3"
        )}
      >
        <div>
          <h2 id={id} className="text-[15px] font-semibold tracking-[0.01em]">
            {title}
          </h2>
          {sub ? (
            <p className="text-[13px] text-muted-foreground">{sub}</p>
          ) : null}
        </div>
        {action}
      </div>
      {children}
    </Card>
  )
}

export function GhostLink({
  href,
  children,
}: {
  href: string
  children: React.ReactNode
}) {
  return (
    <Link
      href={href}
      className={buttonVariants({ variant: "ghost", size: "sm" })}
    >
      {children}
    </Link>
  )
}
