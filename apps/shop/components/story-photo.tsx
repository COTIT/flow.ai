"use client"

import { useState, type ReactNode } from "react"

export function StoryPhoto({
  photo,
  className,
  fallback,
}: {
  photo: { src: string; width: number; height: number; alt: string }
  className: string
  fallback: ReactNode
}) {
  const [failed, setFailed] = useState(false)

  if (failed) return fallback

  return (
    <img
      {...photo}
      className={className}
      loading="lazy"
      decoding="async"
      onError={() => setFailed(true)}
      ref={(node) => {
        // An image can fail before hydration attaches its error handler.
        if (node?.complete && !node.naturalWidth) setFailed(true)
      }}
    />
  )
}
