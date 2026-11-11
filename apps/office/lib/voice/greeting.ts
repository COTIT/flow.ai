import "server-only"
import { voiceSetting } from "./settings"
import type { VoiceContext } from "./server"

/** What the browser knows at start: the user opened with a typed command, or a task can resume. */
export type GreetingHint = "command" | "resume"
export const GREETING_HINTS: readonly GreetingHint[] = ["command", "resume"]

type Language = "en" | "nl" | "sw"
const TEXT: Record<
  Language,
  { resume: string; one: string; many: (n: number) => string; open: string; ask: string }
> = {
  en: {
    resume: "Welcome back. Your earlier task is still here. Shall we continue?",
    open: "Hi, welcome back.",
    one: "One thing needs your attention.",
    many: (n) => `${n} things need your attention.`,
    ask: "Where shall we start?",
  },
  nl: {
    resume: "Welkom terug. Je eerdere taak staat nog klaar. Zullen we verdergaan?",
    open: "Hallo, welkom terug.",
    one: "Er vraagt één ding om je aandacht.",
    many: (n) => `Er vragen ${n} dingen om je aandacht.`,
    ask: "Waar beginnen we?",
  },
  sw: {
    resume: "Karibu tena. Kazi yako ya awali bado ipo. Tuendelee?",
    open: "Habari, karibu tena.",
    one: "Kuna jambo moja linalohitaji umakini wako.",
    many: (n) => `Kuna mambo ${n} yanayohitaji umakini wako.`,
    ask: "Tuanze wapi?",
  },
}

/**
 * The agent's first message, or undefined for the agent's own static greeting.
 * "" = say nothing first: the user's typed command is the first turn.
 * Spoken aloud, so it names a count only, never a record, name or amount.
 */
export function greetingText({
  language,
  hint,
  attention = 0,
}: {
  language: string
  hint?: GreetingHint
  attention?: number
}): string | undefined {
  if (hint === "command") return ""
  const text = TEXT[(language in TEXT ? language : "en") as Language]
  if (hint === "resume") return text.resume
  if (attention < 1) return undefined
  return `${text.open} ${attention === 1 ? text.one : text.many(attention)} ${text.ask}`
}

/** Not-snoozed Flow noticed groups the caller may see (their own bearer); undefined when unreadable. */
async function attentionCount(
  context: Pick<VoiceContext, "apiUrl" | "token" | "workspaceId">
): Promise<number | undefined> {
  try {
    const response = await fetch(
      `${context.apiUrl}/workspace/${encodeURIComponent(context.workspaceId)}/attention/priorities`,
      {
        redirect: "error",
        cache: "no-store",
        signal: AbortSignal.timeout(3000),
        headers: {
          Accept: "application/json",
          Authorization: `Bearer ${context.token}`,
        },
      }
    )
    if (!response.ok) return undefined
    const groups = ((await response.json()) as { data?: { groups?: unknown } })
      .data?.groups
    if (!Array.isArray(groups)) return undefined
    return groups.filter(
      (group) => group && typeof group === "object" && group.snoozed === false
    ).length
  } catch {
    return undefined
  }
}

/**
 * Off unless ASSISTANT_GREETING_OVERRIDE=on: the published agent must allow the first_message
 * override (agent template, D9 run) or ElevenLabs refuses the conversation.
 */
export async function sessionGreeting(
  context: Pick<VoiceContext, "apiUrl" | "token" | "workspaceId">,
  language: string,
  hint: GreetingHint | undefined
): Promise<string | undefined> {
  if ((await voiceSetting("ASSISTANT_GREETING_OVERRIDE")) !== "on") return
  if (hint) return greetingText({ language, hint })
  const attention = await attentionCount(context)
  return attention === undefined ? undefined : greetingText({ language, attention })
}
