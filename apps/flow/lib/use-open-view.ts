"use client"

import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import type { OpenView } from "@genesy/voice/lib/open-view"
import { useScenes, useSceneTitle } from "@/components/scenes/scene-view"
import { flowStore } from "@/lib/flow-store"
import { resolveView, type View } from "@/lib/open-view"
import { BUILT_IN } from "@/lib/scenes"

/**
 * What the assistant's `open_flow_view` does in Flow (P4): open a view from the allow-list, the way a
 * tap would (home exactly like Back to home). Navigation only: never a write, never another origin.
 */
export function useOpenView(): OpenView {
  const t = useTranslations()
  const router = useRouter()
  const list = useScenes()
  const sceneTitle = useSceneTitle()
  return (asked) => {
    const scenes = list.data?.data ?? []
    const keys = new Set([...BUILT_IN, ...scenes.map((s) => s.scene)])
    const views: View[] = [
      { view: "home", title: t("shell.backHome"), path: "/" },
      ...[...keys].map((key) => {
        const scene = scenes.find((s) => s.scene === key)
        const title = scene ? sceneTitle(scene) : t(`scenes.scene.${key}.title`)
        return { view: key, title, path: `/scenes/${key}` }
      }),
      {
        view: "storefront",
        title: t("flow.chat.storefront"),
        path: "/storefront",
      },
      {
        view: "explore",
        title: t("discovery.hint.whatCanDo"),
        path: "/explore",
      },
    ]
    const found = resolveView(asked, views)
    if (!found)
      return {
        status: "not_found",
        available: views.map(({ view, title }) => ({ view, title })),
      }
    const { view, title, path } = found
    if (location.pathname === path && !flowStore.getState().resultOpen)
      return { status: "already_open", view, title }
    // Leaving for a view closes results and the conversation view, as Back to home does.
    flowStore.setState({ resultOpen: false, immersive: false })
    router.push(path)
    return { status: "opened", view, title }
  }
}
