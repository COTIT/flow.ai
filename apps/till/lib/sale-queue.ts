import { createJSONStorage, persist } from "zustand/middleware"
import { createStore } from "zustand/vanilla"
import { useStore } from "zustand"

import {
  tillSalesCreate,
  tillSalesGet,
} from "@verzo/api-client/endpoints/till"
import type { SaleRequest, SaleResource } from "@verzo/api-client/model"
import type { CartLine } from "@/lib/cart-store"
import { migrateQueue, whereFrom, type Origin } from "@/lib/queue-origin"
import { tillStore } from "@/lib/till-store"

export type QueuedSale = Origin & {
  /** Shown while the sale waits for its own register or till user. */
  registerName?: string
  userName?: string
  body: SaleRequest
  /** The lines as rung up, for the receipt once the server has the sale. */
  lines: CartLine[]
  status: "pending" | "done" | "conflict" | "review"
  /** The API's message for a conflict or review. */
  message?: string
  /** The API's code for it, when it sent one (`insufficient_stock`). */
  code?: string
  sale?: SaleResource
}

type Queue = {
  items: QueuedSale[]
  syncing: boolean
  add: (item: Omit<QueuedSale, "status">) => void
  dismiss: (clientRef: string) => void
  /** Sends a sale held for review again, under its own client_ref (after the office fixed it). */
  retry: (clientRef: string) => void
}

/** Sales rung up while the API couldn't be reached, kept until the server has each once. */
export const queueStore = createStore<Queue>()(
  persist(
    (set, get) => ({
      items: [],
      syncing: false,
      add: (item) => {
        set({ items: [...get().items, { ...item, status: "pending" }] })
        void replay()
      },
      dismiss: (ref) =>
        set({ items: get().items.filter((i) => i.body.client_ref !== ref) }),
      retry: (ref) => {
        settle(ref, { status: "pending", message: undefined, code: undefined })
        void replay()
      },
    }),
    {
      name: "genesy.till.queue",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ items }) => ({ items }),
      version: 1,
      migrate: (persisted, version) =>
        migrateQueue(persisted, version) as Pick<Queue, "items">,
      // Read storage after the first render (TillProvider), so server and browser HTML match.
      skipHydration: true,
    }
  )
)

export function useQueue<T>(pick: (s: Queue) => T): T {
  return useStore(queueStore, pick)
}

export const pendingCount = (s: Queue) =>
  s.items.filter((i) => i.status === "pending").length

/** The queued sale's origin against who is logged in on which register now. */
export function whereNow(item: QueuedSale) {
  const { device, session } = tillStore.getState()
  return whereFrom(item, {
    workspace: device?.workspaceId,
    registerId: session?.register.id,
    tillUserId: session?.user.id,
  })
}

/** Waiting sales whose cash is in this register's drawer, whoever rang them up. */
export const pendingHere = (s: Queue) =>
  s.items.filter(
    (i) => i.status === "pending" && whereNow(i) !== "otherRegister"
  ).length

type HttpError = {
  response?: { status: number; data?: { message?: string; code?: string } }
}

/** The API's 409 when a cash sale finds no open shift: retried once the drawer is open. */
const NO_SHIFT = "No shift is open on this register: open the drawer first."

function settle(ref: string, patch: Partial<QueuedSale>) {
  queueStore.setState((s) => ({
    items: s.items.map((i) =>
      i.body.client_ref === ref ? { ...i, ...patch } : i
    ),
  }))
}

/**
 * Sends each pending sale once, oldest first. First asks the server whether it already has
 * the `client_ref` (an answer that got lost), and only posts when it doesn't; the server
 * books one sale per `client_ref` either way. The server books under the session's register and
 * till user, so only a sale rung up by both is posted: another till user's waits for them, another
 * register's is never asked for (its refs are unknown here), and one of unknown origin is only
 * looked up. Stops at the first sign of no network or no login.
 */
async function replayAll() {
  for (const item of queueStore.getState().items) {
    if (item.status !== "pending") continue
    const where = whereNow(item)
    if (where === "otherRegister") continue
    const ref = item.body.client_ref
    try {
      let sale: SaleResource
      try {
        sale = (await tillSalesGet(item.workspace, ref)).data
      } catch (e) {
        if ((e as HttpError).response?.status !== 404 || where !== "here")
          throw e
        sale = (await tillSalesCreate(item.workspace, item.body)).data
      }
      settle(ref, { status: "done", sale, message: undefined, code: undefined })
    } catch (e) {
      const res = (e as HttpError).response
      // Not on this register (yet): it waits for its own till user, or for the office.
      if (res?.status === 404 && where !== "here") continue
      // No answer (offline again), the session ended, the server is down, or the session's
      // workspace changed under the call: try later, as is.
      if (!res || res.status === 401 || res.status === 404 || res.status >= 500)
        return
      if (res.status === 409 && res.data?.message === NO_SHIFT) {
        settle(ref, { message: NO_SHIFT })
        continue
      }
      // Final until someone acts: never retried on its own, never under another client_ref.
      settle(ref, {
        status: res.status === 409 ? "conflict" : "review",
        message: res.data?.message,
        code: res.data?.code,
      })
    }
  }
}

/** Replays the queue unless a run is already going, in this tab or another. */
export async function replay() {
  if (!navigator.onLine || queueStore.getState().syncing) return
  queueStore.setState({ syncing: true })
  try {
    if (navigator.locks)
      await navigator.locks.request(
        "genesy.till.queue",
        { ifAvailable: true },
        (lock) => (lock ? replayAll() : undefined)
      )
    else await replayAll()
  } finally {
    queueStore.setState({ syncing: false })
  }
}
