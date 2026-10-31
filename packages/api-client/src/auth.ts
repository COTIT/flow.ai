"use client"

import { useQueryClient } from "@tanstack/react-query"
import { useStore } from "zustand"

import { useAuthLogin, useAuthLogout } from "./generated/auth/auth"
import { sessionStore, type Session } from "./session"

/** Reads the session in a component and re-renders when the picked part changes. */
export function useSession<T>(pick: (s: Session) => T): T {
  return useStore(sessionStore, pick)
}

/** Login mutation that keeps the token on success. */
export function useLogin() {
  return useAuthLogin({ mutation: { onSuccess: ({ data }) => sessionStore.getState().signIn(data) } })
}

/** Logout mutation: ends the token on the backend, then forgets it here even if that call fails. */
export function useLogout() {
  const queryClient = useQueryClient()
  return useAuthLogout({
    mutation: {
      onSettled: () => {
        sessionStore.getState().signOut()
        queryClient.clear()
      },
    },
  })
}
