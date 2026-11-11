import { useQuery } from "@tanstack/react-query"

import { readinessState, type Readiness } from "./readiness"

/** The assistant's readiness, asked once and shared by every part of the shell. */
export function useReadiness() {
  const query = useQuery({
    queryKey: ["voice-readiness"],
    retry: false,
    queryFn: async () => {
      const response = await fetch("/api/voice/readiness", { cache: "no-store" })
      const body: unknown = await response.json().catch(() => undefined)
      return readinessState(response.status, body)
    },
  })
  const state: Readiness | undefined = query.isError ? "unknown" : query.data
  return { query, state }
}
