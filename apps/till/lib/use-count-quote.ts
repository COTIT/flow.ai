"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import { tillShiftCountQuote } from "@genesy/api-client/endpoints/till"
import type { CountLine } from "@genesy/api-client/model"
import { useOnline } from "@/lib/use-online"
import { useTill } from "@/lib/till-store"

/** Wait this long after the last tap before asking the API. */
const DEBOUNCE_MS = 300

/**
 * The API's figures for a count while the cashier counts (`POST /till/shift/count-quote`,
 * books nothing): total, and with a shift open, expected and the difference, in minor units.
 * The till adds nothing up; offline (or with an incomplete row) there are no figures.
 */
export function useCountQuote(count: CountLine[] | null) {
  const workspace = useTill((s) => s.device?.workspaceId) ?? ""
  const online = useOnline()
  const [settled, setSettled] = React.useState(count)
  const key = JSON.stringify(count)
  React.useEffect(() => {
    const timer = setTimeout(() => setSettled(count), DEBOUNCE_MS)
    return () => clearTimeout(timer)
    // The rows are rebuilt every render; their JSON is what changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])
  const query = useQuery({
    queryKey: ["till-count-quote", workspace, settled],
    queryFn: ({ signal }) =>
      tillShiftCountQuote(
        workspace,
        { count: settled! },
        undefined,
        signal
      ).then((r) => r.data),
    enabled: online && settled != null && settled.length > 0,
    retry: false,
    staleTime: 30_000,
  })
  const current = settled != null && JSON.stringify(settled) === key
  return { online, quote: online && current ? query.data : undefined }
}
