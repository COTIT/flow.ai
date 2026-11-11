"use client"

import { Card, CardContent } from "@genesy/ui/components/card"
import { ScreenError } from "@/components/screen-error"

/** Last resort, outside the signed-in frame (e.g. the login page): one card, a retry and a way home. */
export default function ErrorPage(props: { error: Error; reset: () => void }) {
  return (
    <main className="flex min-h-svh flex-col justify-center p-4">
      <Card className="mx-auto w-full max-w-lg">
        <CardContent>
          <ScreenError {...props} />
        </CardContent>
      </Card>
    </main>
  )
}
