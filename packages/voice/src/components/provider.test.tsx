import { renderToStaticMarkup } from "react-dom/server"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { GenesyVoiceProvider } from "./provider"

const seen = vi.hoisted(() => ({ options: undefined as Record<string, unknown> | undefined }))

vi.mock("@elevenlabs/react", () => ({
  ConversationProvider: ({ children }: { children: React.ReactNode }) => children,
  useRawConversation: () => null,
  useConversation: (options: Record<string, unknown>) => {
    seen.options = options
    return { status: "disconnected", isSpeaking: false, getInputVolume: () => 0, getOutputVolume: () => 0 }
  },
}))
vi.mock("next-intl", () => ({
  useTranslations: () => Object.assign((key: string) => key, { has: () => false }),
  useLocale: () => "en",
}))
vi.mock("@tanstack/react-query", () => ({ useQueryClient: () => ({}) }))
vi.mock("@genesy/api-client/auth", () => ({
  useSession: (pick: (s: { workspaceId: string; token: string }) => unknown) =>
    pick({ workspaceId: "w1", token: "t1" }),
}))

beforeEach(() => {
  seen.options = undefined
})

describe("GenesyVoiceProvider audio worklets", () => {
  it("loads them from Office's /voice-audio by default", () => {
    renderToStaticMarkup(<GenesyVoiceProvider>x</GenesyVoiceProvider>)
    expect(seen.options).toMatchObject({
      workletPaths: {
        rawAudioProcessor: "/voice-audio/rawAudioProcessor.js",
        audioConcatProcessor: "/voice-audio/audioConcatProcessor.js",
      },
      libsampleratePath: "/voice-audio/libsamplerate.worklet.js",
    })
  })

  it("loads them from the host's audioBase when given", () => {
    renderToStaticMarkup(<GenesyVoiceProvider audioBase="/assets/voice">x</GenesyVoiceProvider>)
    expect(seen.options).toMatchObject({
      workletPaths: {
        rawAudioProcessor: "/assets/voice/rawAudioProcessor.js",
        audioConcatProcessor: "/assets/voice/audioConcatProcessor.js",
      },
      libsampleratePath: "/assets/voice/libsamplerate.worklet.js",
    })
  })
})
