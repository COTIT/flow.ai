"use client"

import { notFound } from "next/navigation"
import {
  AlertTriangle,
  ChevronRight,
  Layers,
  ShoppingBag,
  Wallet,
  type LucideIcon,
} from "lucide-react"
import MagicBento from "@/components/rb/MagicBento"
import { Chip } from "@/components/bento"

/** Design preview of the overview cards with the Magic Bento glow; development builds only. */
function Kpi({
  icon,
  tone,
  total,
  label,
}: {
  icon: LucideIcon
  tone: "teal" | "gold" | "red" | "slate"
  total: string
  label: string
}) {
  return (
    <div className="flex h-full flex-col gap-2 p-6 max-md:p-4">
      <span className="flex items-center justify-between">
        <Chip icon={icon} tone={tone} />
        <ChevronRight aria-hidden className="size-4 text-muted-foreground" strokeWidth={1.75} />
      </span>
      <span className="block text-[34px] leading-[1.1] font-semibold max-md:text-[28px]">
        {total}
      </span>
      <span className="text-[13px] text-muted-foreground">{label}</span>
    </div>
  )
}

function Tile({ title, sub, rows }: { title: string; sub: string; rows: [string, string][] }) {
  return (
    <div className="flex h-full flex-col gap-4 p-6 max-md:gap-3 max-md:p-4">
      <div>
        <h2 className="text-[15px] font-semibold tracking-[0.01em]">{title}</h2>
        <p className="text-[13px] text-muted-foreground">{sub}</p>
      </div>
      <ul className="flex flex-col divide-y divide-(--hair) text-sm">
        {rows.map(([a, b]) => (
          <li key={a} className="flex justify-between gap-3 py-2">
            <span className="truncate">{a}</span>
            <span className="shrink-0 text-muted-foreground">{b}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export default function OverviewDesignPage() {
  if (process.env.NODE_ENV === "production") notFound()
  return (
    <MagicBento
      columns={4}
      rowHeight={150}
      gap={16}
      radius={18}
      theme="dark"
      glowColor="#efc977"
      enableBorderGlow
      enableSpotlight
      spotlightRadius={320}
      enableStars={false}
      enableTilt={false}
      enableMagnetism={false}
      clickEffect
      textAutoHide={false}
      items={[
        { id: "k1", colSpan: 1, content: <Kpi icon={ShoppingBag} tone="teal" total="12" label="Orders to pick" /> },
        { id: "k2", colSpan: 1, content: <Kpi icon={Layers} tone="gold" total="3" label="Lots in quarantine" /> },
        { id: "k3", colSpan: 1, content: <Kpi icon={AlertTriangle} tone="red" total="1" label="Stock below zero" /> },
        { id: "k4", colSpan: 1, content: <Kpi icon={Wallet} tone="slate" total="0" label="Till mismatches" /> },
        {
          id: "orders",
          colSpan: 3,
          rowSpan: 2,
          content: (
            <Tile
              title="Recent orders"
              sub="Last 7 days"
              rows={[
                ["#1042 Mama Lishe Café", "TSh 86,500"],
                ["#1041 Kilimanjaro Lodge", "TSh 212,000"],
                ["#1040 Walk-in", "TSh 9,800"],
                ["#1039 Ruaha Market", "TSh 54,300"],
              ]}
            />
          ),
        },
        {
          id: "attention",
          colSpan: 1,
          rowSpan: 2,
          content: (
            <Tile
              title="Needs attention"
              sub="Today"
              rows={[
                ["Lot RH-2026-10", "held"],
                ["Invoice 2026-118", "overdue"],
              ]}
            />
          ),
        },
        { id: "till", colSpan: 2, content: <Tile title="Till today" sub="Main register" rows={[["Sales", "TSh 318,400"], ["Shift", "open 08:02"]]} /> },
        { id: "lots", colSpan: 1, content: <Tile title="Lots by status" sub="All places" rows={[["Released", "41"], ["Quarantine", "3"]]} /> },
        { id: "sales", colSpan: 1, content: <Tile title="Sales this week" sub="vs last week" rows={[["Total", "TSh 1.42M"], ["Change", "+8%"]]} /> },
      ]}
    />
  )
}
