"use client"

import AppErrorPage from "./(app)/error"

/** A screen outside the shell that crashed: the same card, centred on the page. */
export default function ErrorPage(props: { error: Error; reset: () => void }) {
  return (
    <main className="flex min-h-svh flex-col justify-center p-4">
      <AppErrorPage {...props} />
    </main>
  )
}
