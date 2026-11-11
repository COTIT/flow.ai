"use client"

import { useEffect, useMemo, useState } from "react"
import { usePathname, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { useStore } from "zustand"
import { createStore } from "zustand/vanilla"

import { useSession } from "@genesy/api-client/auth"
import { useAttentionPriorities } from "@genesy/api-client/endpoints/attention"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { useRights } from "@genesy/api-client/rights"
import { useVoice } from "@genesy/voice/components/provider"
import {
  FLOW_CAPABILITIES,
  sceneKey,
  statusOf,
  type FlowCapability,
} from "@genesy/voice/lib/capabilities"
import { nextActions, rankPills } from "@genesy/voice/lib/rank"
import { activityKey, readActivity, recordUse } from "./activity"

/** A suggestion: a capability id (stable key) and what it does. */
export type Pill = { id: string; label: string; run: () => void }

const BY_ID = new Map(FLOW_CAPABILITIES.map((c) => [c.id, c]))
export const capability = (id: string) => BY_ID.get(id)!

/** Bumped by "Reset suggestions" so the pills re-rank without waiting for navigation. */
export const activityVersion = createStore<{ n: number; bump: () => void }>()(
  (set) => ({ n: 0, bump: () => set((s) => ({ n: s.n + 1 })) })
)

/** The dock's send (registered by the dock's suggestions), so the explorer sends the same way. */
export const dockAsk = createStore<{ ask?: (text: string) => void }>()(
  () => ({})
)

/** This person's activity key in this workspace (null until both are known). */
export function useActivityKey() {
  const workspaceId = useSession((s) => s.workspaceId)
  const token = useSession((s) => s.token)
  const me = useUsersMe({ query: { enabled: Boolean(token) } })
  const userId = me.data?.data.user.id
  return userId && workspaceId ? activityKey(userId, workspaceId) : null
}

/**
 * Runs a capability like the equivalent instruction: a screen opens directly (no AI, no
 * session); an instruction goes to Genesy through the dock (`ask`), as typing it would.
 */
export function useRunCapability(ask: (text: string) => void) {
  const t = useTranslations("discovery.capabilities")
  const router = useRouter()
  const key = useActivityKey()
  return (cap: FlowCapability) => {
    if ("href" in cap.entry) return router.push(cap.entry.href)
    if (key) recordUse(key, cap.id)
    ask(t(`${cap.id}.prompt`))
  }
}

/**
 * The dock's suggestions. Re-ranked only on meaningful changes (scene, rights, Flow noticed,
 * a completed action, a reset): using a pill never reorders what is on screen.
 */
export function usePills(ask: (text: string) => void): {
  next: boolean
  pills: Pill[]
} {
  const t = useTranslations("discovery.capabilities")
  const voice = useVoice()
  const pathname = usePathname()
  const rights = useRights()
  const key = useActivityKey()
  const workspaceId = useSession((s) => s.workspaceId)
  const version = useStore(activityVersion, (s) => s.n)
  const run = useRunCapability(ask)
  const scene = sceneKey(pathname)

  const priorities = useAttentionPriorities(workspaceId ?? "", {
    query: { enabled: Boolean(workspaceId) },
  })
  const attention = (priorities.data?.data.groups ?? [])
    .filter((g) => !g.snoozed && g.open_count > 0)
    .map((g) => g.kind)
    .join()
  const allowed = rights.read
    ? FLOW_CAPABILITIES.map((c) => statusOf(c, rights.may)).join()
    : ""

  // A visited scene counts as a use of the capability that opens it.
  useEffect(() => {
    const visited = FLOW_CAPABILITIES.find(
      (c) =>
        "href" in c.entry &&
        c.entry.href !== "/#your-flow" &&
        c.entry.href === pathname
    )
    if (key && visited) recordUse(key, visited.id)
  }, [key, pathname])

  // A completed action counts for the capabilities running it and offers what comes next.
  const sent = voice.messages.filter((m) => m.role === "user").length
  const outcome = voice.outcome
  const op = outcome?.status === "completed" ? outcome.operationId : undefined
  const [done, setDone] = useState<{ op: string; at: string; sent: number }>()
  const [seen, setSeen] = useState(outcome)
  if (outcome !== seen) {
    setSeen(outcome)
    if (op) setDone({ op, at: pathname, sent })
  }
  useEffect(() => {
    if (key && op)
      for (const c of FLOW_CAPABILITIES.filter((c) => c.ops.includes(op)))
        recordUse(key, c.id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [outcome])

  const ids = useMemo(
    () =>
      allowed
        ? rankPills({
            list: FLOW_CAPABILITIES,
            scene,
            may: rights.may,
            uses: key ? readActivity(key).uses : {},
            attention: attention ? attention.split(",") : [],
          })
        : [],
    // Activity is read here, not watched: recording a use must not move pills under the pointer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [scene, allowed, attention, key, version, done]
  )

  const following =
    done && done.at === pathname && done.sent === sent && rights.read
      ? nextActions(FLOW_CAPABILITIES, done.op, rights.may).map((c) => c.id)
      : []
  const shown = following.length ? following : ids
  return {
    next: following.length > 0,
    pills: shown.map((id) => ({
      id,
      label: t(`${id}.label`),
      run: () => run(capability(id)),
    })),
  }
}
