/** One request at a time with a single waiting slot (P6-2). A call that arrives while one runs
 * waits and goes next; a newer call takes the slot from an older waiting one, which is
 * superseded at once; a waiting call whose conversation moved on is cancelled, never run. */
export function oneSlot() {
  let running: Promise<void> | null = null
  let waiting: ((turn: "superseded") => void) | null = null
  return {
    /** Marks a request as running; call the returned function once it has settled. */
    begin() {
      let done = () => {}
      const settled = new Promise<void>((resolve) => (done = resolve))
      running = settled
      return () => {
        if (running === settled) running = null
        done()
      }
    },
    wait(current: () => boolean): Promise<"run" | "superseded" | "cancelled"> {
      waiting?.("superseded")
      const settled = running ?? Promise.resolve()
      return new Promise((resolve) => {
        const mine = resolve
        waiting = mine
        void settled.then(() => {
          if (waiting !== mine) return
          waiting = null
          resolve(current() ? "run" : "cancelled")
        })
      })
    },
  }
}
