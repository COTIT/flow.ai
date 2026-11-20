import { cn } from "@verzo/ui/lib/utils"

import { card } from "@/lib/look"

/** Mock `ShopOrder`: one card, centred at 920 px. The order page and its error share it. */
export function OrderCard({ children }: { children: React.ReactNode }) {
  return (
    <section
      className={cn(
        card,
        "afm-order mx-auto w-full max-w-[920px] gap-[18px] md:gap-[18px]"
      )}
    >
      {children}
    </section>
  )
}
