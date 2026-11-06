import { cn } from "@genesy/ui/lib/utils"

import { card, prose, tag, warm } from "@/lib/look"

/** A content page's opening card: small tag, the page's h1, one lead paragraph. */
export function PageIntro({
  label,
  title,
  lead,
  children,
}: {
  label: string
  title: string
  lead: string
  children?: React.ReactNode
}) {
  return (
    <section className={cn(card, warm, "gap-4 p-6 md:gap-5 md:p-10")}>
      <p className={cn(tag, "text-(--tone-warning)")}>{label}</p>
      <h1 className="max-w-[760px] text-[28px] leading-[1.15] font-medium md:text-[40px]">
        {title}
      </h1>
      <p className={cn(prose, "text-muted-foreground")}>{lead}</p>
      {children}
    </section>
  )
}
