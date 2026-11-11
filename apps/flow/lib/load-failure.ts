// Chromium, Firefox and Safari wordings for code or data that never arrived.
const LOAD =
  /^(Loading (CSS )?chunk \S+ failed|Failed to load chunk |Failed to fetch dynamically imported module|Importing a module script failed|Failed to fetch RSC payload|Failed to fetch$|Load failed$|NetworkError when attempting to fetch resource)/

/** A page whose code or data did not arrive (the network), as opposed to a page that crashed. */
export function isLoadFailure(error: unknown): boolean {
  return error instanceof Error && (error.name === "ChunkLoadError" || LOAD.test(error.message))
}
