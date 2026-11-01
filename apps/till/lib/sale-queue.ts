import { createJSONStorage, persist } from "zustand/middleware"
import { createStore } from "zustand/vanilla"
import { useStore } from "zustand"

import {
  tillSalesCreate,
  tillSalesGet,
} from "@genesy/api-client/endpoints/till"
import type { SaleRequest, SaleResource } from "@genesy/api-client/model"
import type { CartLine } from "@/lib/cart-store"

export type QueuedSale = {
  workspace: string
  body: SaleRequest
  /** The lines as rung up, for the receipt once the server has the sale. */
  lines: CartLine[]
  status: "pending" | "done" | "conflict" | "review"
  /** The API's message for a conflict or review. */
  message?: string
  sale?: SaleResource
}

type Queue = {
  items: QueuedSale[]
  syncing: boolean
  add: (item: Omit<QueuedSale, "status">) => void
  dismiss: (clientRef: string) => void
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
    }),
    {
      name: "genesy.till.queue",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ items }) => ({ items }),
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

type HttpError = { response?: { status: number; data?: { message?: string } } }

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
 * books one sale per `client_ref` either way. Stops at the first sign of no network or no login.
 * shortcut: the sale is booked to whoever is logged in when it replays (the server takes the till
 * user from the session); send the original till user once the API accepts one in the body.
 */
async function replayAll() {
  for (const item of queueStore.getState().items) {
    if (item.status !== "pending") continue
    const ref = item.body.client_ref
    try {
      let sale: SaleResource
      try {
        sale = (await tillSalesGet(item.workspace, ref)).data
      } catch (e) {
        if ((e as HttpError).response?.status !== 404) throw e
        sale = (await tillSalesCreate(item.workspace, item.body)).data
      }
      settle(ref, { status: "done", sale })
    } catch (e) {
      const res = (e as HttpError).response
      // No answer (offline again), the session ended or the server is down: try later, as is.
      if (!res || res.status === 401 || res.status >= 500) return
      settle(ref, {
        status: res.status === 409 ? "conflict" : "review",
        message: res.data?.message,
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
