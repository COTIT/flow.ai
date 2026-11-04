"use client"

import { useTranslations } from "next-intl"

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import { useTillMoney } from "@/lib/till-money"

/** The till lists' register filter ("" = all registers), in the card header. */
export function RegisterSelect({
  value,
  onChange,
}: {
  value: string
  onChange: (registerId: string) => void
}) {
  const t = useTranslations()
  const { registers } = useTillMoney()
  const items = [
    { value: "", label: t("till.allRegisters") },
    ...registers.map((r) => ({ value: r.id, label: r.name })),
  ]
  return (
    <Select
      items={items}
      value={value}
      onValueChange={(v) => onChange(v ?? "")}
    >
      <SelectTrigger
        aria-label={t("till.register")}
        className="w-[200px] max-md:w-full"
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {items.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
