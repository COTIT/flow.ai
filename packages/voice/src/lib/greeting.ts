/** Opening the browser knows about: the user started by typing a command. */
export type Opening = "command"

/** Hint for Office's greeting: a typed command wins, then a task that can resume. */
export function greetingHint(opening: Opening | undefined, recoverable: number) {
  return opening ?? (recoverable > 0 ? ("resume" as const) : undefined)
}

/** ElevenLabs agent override; firstMessage only when Office sent one (ASSISTANT_GREETING_OVERRIDE). */
export function agentOverride<L extends string>(language: L, greeting: unknown): { language: L; firstMessage?: string } {
  return typeof greeting === "string" ? { language, firstMessage: greeting } : { language }
}
