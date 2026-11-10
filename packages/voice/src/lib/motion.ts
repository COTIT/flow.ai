export function voiceMotionActive(
  state: {
    visible: boolean
    connected: boolean
    textOnly: boolean
    muted: boolean
    speaking: boolean
    lastInputAt: number | null
  },
  now: number
) {
  return (
    state.visible &&
    state.connected &&
    !state.textOnly &&
    !state.muted &&
    (state.speaking ||
      (state.lastInputAt !== null && now - state.lastInputAt < 1500))
  )
}
