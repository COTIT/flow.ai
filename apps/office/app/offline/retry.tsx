"use client"

import { Button } from "@genesy/ui/components/button"

/** Loads the page the user asked for again (the service worker shows this screen in its place). */
export function Retry({ children }: { children: React.ReactNode }) {
  return (
    <Button className="min-h-11" onClick={() => window.location.reload()}>
      {children}
    </Button>
  )
}
