import { MapPinIcon } from "lucide-react"

import { cn } from "@genesy/ui/lib/utils"

import { card, h2, prose } from "@/lib/look"
import { StoryPhoto } from "./story-photo"

/** "Benard Omondi, "Art Benny"" -> "BO": capitals of the name, nickname left out. */
export const initials = (name: string) =>
  name
    .replace(/".*"/, "")
    .split(/[\s,]+/)
    .filter((w) => /^\p{Lu}/u.test(w))
    .slice(0, 2)
    .map((w) => w[0])
    .join("")

/**
 * One person or group: their photo (AFM's own, sized so nothing shifts) or, without one, a
 * warm tile with their initials; then name, place and story. `wide` lays it side by side
 * from md up.
 */
export function StoryCard({
  name,
  place,
  body,
  wide,
  compact,
  photo,
  as: Tag = "li",
}: {
  name: string
  place: string
  body: string
  wide?: boolean
  /** A small round badge instead of the 4:3 tile (home page teaser). */
  compact?: boolean
  photo?: { src: string; width: number; height: number; alt: string }
  as?: "li" | "article"
}) {
  const frame = cn(
    compact ? "size-14 rounded-full" : "aspect-[4/3] w-full rounded-[16px]",
    wide && "md:w-[42%]"
  )
  const fallback = (
    <span
      aria-hidden
      className={cn(
        "afm-story-placeholder flex shrink-0 items-center justify-center font-semibold text-white",
        "bg-[radial-gradient(120%_90%_at_30%_20%,#e9bf6b_0%,#b98336_45%,#7a4b2a_100%)]",
        compact ? "text-[18px]" : "text-[40px]",
        frame
      )}
    >
      {initials(name)}
    </span>
  )
  return (
    <Tag
      className={cn(
        card,
        "afm-story-card",
        wide && "md:col-span-2 md:flex-row"
      )}
    >
      {photo ? (
        <StoryPhoto
          key={photo.src}
          photo={photo}
          className={cn(
            "afm-story-image shrink-0 object-cover object-[50%_20%]",
            frame
          )}
          fallback={fallback}
        />
      ) : (
        fallback
      )}
      <div className="afm-story-body flex flex-col gap-2">
        <h3 className={h2}>{name}</h3>
        <p className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
          <MapPinIcon aria-hidden className="size-4" />
          {place}
        </p>
        <p className={prose}>{body}</p>
      </div>
    </Tag>
  )
}
