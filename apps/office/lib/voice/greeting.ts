import "server-only"
import { voiceSetting } from "./settings"
import type { VoiceContext } from "./server"

/** What the browser knows at start: the user opened with a typed command, or a task can resume. */
export type GreetingHint = "command" | "resume"
export const GREETING_HINTS: readonly GreetingHint[] = ["command", "resume"]

type Language = "en" | "nl" | "sw"
type Daypart = "morning" | "afternoon" | "evening"
/** [opening, closing question]; "{, }" becomes ", Name" or nothing, "{ }" becomes " Name" or nothing. */
type Phrasing = readonly [string, string]
const TEXT: Record<
  Language,
  {
    resume: string
    one: string
    many: (n: number) => string
    open: Record<Daypart, readonly Phrasing[]>
  }
> = {
  en: {
    resume: "Welcome back. Your earlier task is still here. Shall we continue?",
    one: "One thing needs your attention.",
    many: (n) => `${n} things need your attention.`,
    open: {
      morning: [
        ["Good morning{, }.", "What shall we start with?"],
        ["Morning{, }!", "What's first today?"],
        ["Hi{ }, good morning.", "How can I help?"],
        ["Good morning{ }, nice to hear from you.", "What do you need?"],
      ],
      afternoon: [
        ["Good afternoon{, }.", "What can I do for you?"],
        ["Hi{ }, good afternoon.", "Where shall we pick up?"],
        ["Afternoon{, }!", "What's on your list?"],
        ["Hello{ }, hope the day is going well.", "What do you need?"],
      ],
      evening: [
        ["Good evening{, }.", "What can I help with?"],
        ["Hi{ }, good evening.", "Anything to wrap up today?"],
        ["Evening{, }!", "What shall we look at?"],
        ["Hello{ }, still at it?", "How can I help?"],
      ],
    },
  },
  nl: {
    resume: "Welkom terug. Je eerdere taak staat nog klaar. Zullen we verdergaan?",
    one: "Er vraagt één ding om je aandacht.",
    many: (n) => `Er vragen ${n} dingen om je aandacht.`,
    open: {
      morning: [
        ["Goedemorgen{, }.", "Waar beginnen we mee?"],
        ["Morgen{, }!", "Wat staat er vandaag als eerste op?"],
        ["Hoi{ }, goedemorgen.", "Waarmee kan ik helpen?"],
        ["Goedemorgen{ }, fijn je te horen.", "Wat heb je nodig?"],
      ],
      afternoon: [
        ["Goedemiddag{, }.", "Wat kan ik voor je doen?"],
        ["Hoi{ }, goedemiddag.", "Waar gaan we verder?"],
        ["Middag{, }!", "Wat staat er op je lijst?"],
        ["Hallo{ }, hoe gaat je dag?", "Waarmee kan ik helpen?"],
      ],
      evening: [
        ["Goedenavond{, }.", "Waarmee kan ik helpen?"],
        ["Hoi{ }, goedenavond.", "Nog iets om af te ronden vandaag?"],
        ["Avond{, }!", "Waar kijken we naar?"],
        ["Hallo{ }, nog even bezig?", "Wat heb je nodig?"],
      ],
    },
  },
  sw: {
    resume: "Karibu tena. Kazi yako ya awali bado ipo. Tuendelee?",
    one: "Kuna jambo moja linalohitaji umakini wako.",
    many: (n) => `Kuna mambo ${n} yanayohitaji umakini wako.`,
    open: {
      morning: [
        ["Habari za asubuhi{, }.", "Tuanze na nini?"],
        ["Asubuhi njema{, }!", "Nini kwanza leo?"],
        ["Hujambo{ }, habari za asubuhi.", "Nikusaidie nini?"],
        ["Karibu{ }, asubuhi njema.", "Unahitaji nini?"],
      ],
      afternoon: [
        ["Habari za mchana{, }.", "Nikusaidie nini?"],
        ["Mchana mwema{, }!", "Tuendelee na nini?"],
        ["Hujambo{ }, habari za mchana.", "Una nini kwenye orodha?"],
        ["Karibu{ }, natumaini siku inaenda vizuri.", "Unahitaji nini?"],
      ],
      evening: [
        ["Habari za jioni{, }.", "Nikusaidie nini?"],
        ["Jioni njema{, }!", "Tuangalie nini?"],
        ["Hujambo{ }, habari za jioni.", "Kuna cha kumalizia leo?"],
        ["Karibu{ }, bado unaendelea?", "Unahitaji nini?"],
      ],
    },
  },
}
export const GREETING_VARIANTS = 4

function daypart(hour: number): Daypart {
  return hour >= 5 && hour < 12 ? "morning" : hour >= 12 && hour < 18 ? "afternoon" : "evening"
}

/** Spoken aloud and seen by the agent: only a short, plain given name, else none. */
function spokenName(name: string | undefined): string | undefined {
  const plain = name?.trim()
  return plain && /^\p{L}[\p{L}'-]{0,29}$/u.test(plain) ? plain : undefined
}

/**
 * The agent's first message. "" = say nothing first: the user's typed command is the first turn.
 * Spoken aloud, so besides the caller's own first name it names a count only, never a record or amount.
 */
export function greetingText({
  language,
  hint,
  attention = 0,
  hour = 12,
  variant = 0,
  firstName,
}: {
  language: string
  hint?: GreetingHint
  attention?: number
  hour?: number
  variant?: number
  firstName?: string
}): string {
  if (hint === "command") return ""
  const text = TEXT[(language in TEXT ? language : "en") as Language]
  if (hint === "resume") return text.resume
  const name = spokenName(firstName)
  const [open, ask] = text.open[daypart(hour)][Math.abs(variant) % GREETING_VARIANTS]!
  const fact = attention < 1 ? "" : attention === 1 ? text.one : text.many(attention)
  const opening = open.replace(/\{([^}]*)\}/, (_: string, sep: string) => (name ? sep + name : ""))
  return [opening, fact, ask].filter(Boolean).join(" ")
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

// shortcut: per Office process; a restart or a second replica may repeat a phrasing once. Move to a shared store if Office scales out.
const lastVariant = new Map<string, { variant: number; at: number }>()
const REMEMBER_MS = 12 * 60 * 60 * 1000

/** A phrasing other than this user's last one, so two sessions in a row never open alike. */
function nextVariant(key: string | undefined, now: number): number {
  const last = key ? lastVariant.get(key) : undefined
  const previous = last && now - last.at < REMEMBER_MS ? last.variant : -1
  let variant = Math.floor(Math.random() * GREETING_VARIANTS)
  if (variant === previous) variant = (variant + 1) % GREETING_VARIANTS
  if (key) {
    lastVariant.delete(key)
    lastVariant.set(key, { variant, at: now })
    if (lastVariant.size > 1000) lastVariant.delete(lastVariant.keys().next().value!)
  }
  return variant
}

function hourIn(timeZone: string | undefined, now: Date): number {
  try {
    return Number(
      new Intl.DateTimeFormat("en", { timeZone: timeZone ?? "UTC", hour: "numeric", hourCycle: "h23" }).format(now)
    )
  } catch {
    return now.getUTCHours()
  }
}

/**
 * Off unless ASSISTANT_GREETING_OVERRIDE=on: the published agent must allow the first_message
 * override (agent template; publish it before turning this on) or ElevenLabs refuses the conversation.
 */
export async function sessionGreeting(
  context: Pick<VoiceContext, "apiUrl" | "token" | "workspaceId" | "userId" | "firstName">,
  language: string,
  hint: GreetingHint | undefined,
  timeZone?: string
): Promise<string | undefined> {
  try {
    if ((await voiceSetting("ASSISTANT_GREETING_OVERRIDE")) !== "on") return
    if (hint) return greetingText({ language, hint })
    const attention = await attentionCount(context)
    const now = new Date()
    return greetingText({
      language,
      attention,
      hour: hourIn(timeZone, now),
      variant: nextVariant(context.userId && `${context.userId}:${context.workspaceId}`, now.getTime()),
      firstName: context.firstName,
    })
  } catch {
    // A broken setting must never stop a session; no values (paths, ids) in the log.
    console.warn("voice greeting skipped: ASSISTANT_GREETING_OVERRIDE is unreadable")
    return undefined
  }
}
