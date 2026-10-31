import { LogoIcon } from "@genesy/ui/components/logo"
import { cn } from "@genesy/ui/lib/utils"

/**
 * Product picture. shortcut: the storefront API sends no product images yet, so this is
 * a branded placeholder; switch to the image link once the API returns one.
 */
export function ProductImage({
  label,
  className,
}: {
  label: string
  className?: string
}) {
  return (
    <div
      role="img"
      aria-label={label}
      className={cn(
        "flex aspect-square items-center justify-center bg-foreground dark:bg-secondary",
        className
      )}
    >
      <LogoIcon aria-hidden className="h-1/4 opacity-90" />
    </div>
  )
}
