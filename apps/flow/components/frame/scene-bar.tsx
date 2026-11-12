"use client"

import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { ArrowLeft } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { flowStore, useFlow } from "@/lib/flow-store"
import { TaskCapsule } from "@/components/task/capsule" // F4
import { OrbSlot } from "./orb-layer"

/** Above every scene: the way home and the compact orb's fixed place (it never scrolls). */
export function SceneBar({
  orb = true,
  conversation = null,
}: {
  /** The compact orb's place (absent while the rail or sheet holds it). */
  orb?: boolean
  /** The way back to a hidden conversation rail. */
  conversation?: React.ReactNode
}) {
  const t = useTranslations("shell")
  const pathname = usePathname()
  const onTask = useFlow((s) => s.resultOpen) && pathname === "/"
  const router = useRouter()
  return (
    <div className="flex shrink-0 items-center gap-2 pb-2 [@media(max-height:500px)]:pb-1">
      <Button
        variant="ghost"
        className="min-h-11 shrink-0"
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
      <div className="min-w-0 flex-1">
        {/* F4: the task stays reachable in every scene. On a short screen's task view it only
            repeats the open review and takes the height that review needs (P9-7). */}
        <div
          className={
            // One line (ellipsis) in the bar, never a wrapping block that eats the scene (R1).
            `min-w-0 [&_[role=status]]:flex-nowrap [&_[role=status]>p]:truncate [&_[role=status]>p_.block]:hidden ${onTask ? "[@media(max-height:500px)]:hidden" : ""}`
          }
        >
          <TaskCapsule />
        </div>
      </div>
      {/* The header's always-present way in (mouse and keyboard; touch also has the handle). */}
      <Button
        variant="outline"
        className="min-h-11 shrink-0 rounded-full @max-[600px]:hidden"
        onClick={() =>
          document
            .querySelector<HTMLElement>("#flow-header a, #flow-header button")
            ?.focus()
        }
      >
        {t("menu")}
      </Button>
      {conversation}
      {orb ? <OrbSlot name="compact" className="size-11" /> : null}
    </div>
  )
}
