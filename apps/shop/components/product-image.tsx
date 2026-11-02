// Plain <img>, not next/image: signed file links change on every fetch (see passport-view).
import type { Link } from "@genesy/api-client/model"
import { LogoIcon } from "@genesy/ui/components/logo"
import { cn } from "@genesy/ui/lib/utils"

import { honey } from "@/lib/look"

/**
 * A product photo from the API's signed link, or (no photo) the mockup's warm placeholder
 * with the logo. `alt=""` marks it decorative, e.g. inside a card whose link names the product.
 */
export function ProductImage({
  image,
  alt,
  label,
  className,
}: {
  image: Link | undefined
  alt: string
  /** Read out for the placeholder when the image isn't decorative. */
  label: string
  className?: string
}) {
  if (image)
    return (
      <img
        src={image.url}
        alt={alt}
        referrerPolicy="no-referrer"
        className={cn("block w-full object-cover", className)}
      />
    )
  return (
    <div
      {...(alt
        ? { role: "img", "aria-label": label }
        : { "aria-hidden": true })}
      className={cn("flex items-center justify-center", honey, className)}
    >
      <LogoIcon aria-hidden className="h-auto w-[34%] min-w-0 opacity-90" />
    </div>
  )
}
