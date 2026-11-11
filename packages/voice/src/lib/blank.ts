/** True when a typed message shows nothing: whitespace and zero-width marks only (`trim()` keeps those). */
export function isBlankMessage(text: string) {
  return !text.replace(/[\s\u200b-\u200d\u2060\ufeff]/g, "")
}
