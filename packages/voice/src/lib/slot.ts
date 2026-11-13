/** Serial requests in arrival order. Recheck ownership before each dispatch. */
export function oneSlot() {
  type Waiting = { current: () => boolean; resolve: (release: (() => void) | null) => void }
  let running = false
  const waiting: Waiting[] = []
  const next = () => {
    if (running) return
    const task = waiting.shift()
    if (!task) return
    if (!task.current()) {
      task.resolve(null)
      next()
      return
    }
    running = true
    let released = false
    task.resolve(() => {
      if (released) return
      released = true
      running = false
      next()
    })
  }
  return {
    acquire(current: () => boolean): Promise<(() => void) | null> {
      return new Promise((resolve) => {
        waiting.push({ current, resolve })
        next()
      })
    },
  }
}
