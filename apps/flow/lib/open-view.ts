/** A place in Flow the assistant may open (P4): its key, its title in the person's language, its path. */
export type View = { view: string; title: string; path: string }

/** Lowercase words only: accents, punctuation and a leading "my"/"the" don't count. */
const words = (text: string) =>
  text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .replace(/^(my|the) /, "")

/**
 * The view the assistant asked for, by key or by title, from the allow-list only: never a raw path,
 * never another origin (the assistant's words are untrusted).
 */
export function resolveView(
  asked: string,
  views: readonly View[]
): View | null {
  const wanted = words(asked)
  if (!wanted) return null
  return (
    views.find((v) => v.view === asked) ??
    views.find((v) => words(v.view) === wanted || words(v.title) === wanted) ??
    null
  )
}
