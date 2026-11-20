"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import type { Link } from "@verzo/api-client/model"
import { cn } from "@verzo/ui/lib/utils"

import { Glare } from "./motion"
import { ProductImage } from "./product-image"

/** Mock `ShopProduct` photo tile: the big photo, and thumbnails that switch it when there are more. */
export function ProductGallery({
  images,
  alt,
}: {
  images: Link[]
  alt: string
}) {
  const t = useTranslations("product")
  const [shown, setShown] = React.useState(0)
  return (
    <>
      <div className="afm-gallery-frame contents">
        <Glare radius="16px">
          <ProductImage
            image={images[shown]}
            alt={alt}
            label={t("imageSoon")}
            className="afm-gallery-image min-h-[460px] flex-1 rounded-[16px]"
          />
        </Glare>
      </div>
      {images.length > 1 ? (
        <div className="grid grid-cols-4 gap-2">
          {images.map((image, i) => (
            <button
              key={image.url}
              type="button"
              aria-label={t("photo", { n: i + 1, total: images.length })}
              aria-pressed={i === shown}
              onClick={() => setShown(i)}
              className={cn(
                "afm-gallery-thumbnail aspect-square overflow-hidden rounded-[16px] outline-offset-2 focus-visible:outline-3 focus-visible:outline-ring",
                i === shown && "outline-2 outline-ring"
              )}
            >
              <ProductImage
                image={image}
                alt=""
                label=""
                className="size-full"
              />
            </button>
          ))}
        </div>
      ) : null}
    </>
  )
}
