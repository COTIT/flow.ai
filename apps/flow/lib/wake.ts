const WAKE = /^\s*hey[\s,]+genesy(?![\p{L}\p{N}])[\s,.!:;-]*/iu

/** Typed "Hey Genesy, …" starts a conversation and sends the rest; it is text, not a wake word. */
export function parseWake(text: string) {
  const match = WAKE.exec(text)
  return match
    ? { wake: true, rest: text.slice(match[0].length).trim() }
    : { wake: false, rest: text }
}
