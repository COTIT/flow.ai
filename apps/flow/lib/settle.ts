/** Waits for `work` to settle, but never longer than `ms`: a hung call must not block what follows. */
export function settleWithin(
  work: Promise<unknown>,
  ms: number
): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms)
    void work
      .catch(() => undefined)
      .finally(() => {
        clearTimeout(timer)
        resolve()
      })
  })
}
