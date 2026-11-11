"use client"

import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { ArrowLeft } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { flowStore } from "@/lib/flow-store"
import { OrbSlot } from "./orb-layer"

/** Above every scene: the way home and the compact orb's fixed place (it never scrolls). */
export function SceneBar() {
  const t = useTranslations("shell")
  const pathname = usePathname()
  const router = useRouter()
  return (
    <div className="flex shrink-0 items-center gap-2 pb-2">
      <Button
        variant="ghost"
        className="mr-auto min-h-11"
        onClick={() => {
          // Closing results returns to the conversation; leaving a scene returns to home.
          if (pathname === "/") return flowStore.setState({ resultOpen: false })
          flowStore.setState({ resultOpen: false, immersive: false })
          router.push("/")
        }}
      >
        <ArrowLeft aria-hidden />
        {t("backHome")}
      </Button>
      <OrbSlot name="compact" className="size-11" />
    </div>
  )
}
