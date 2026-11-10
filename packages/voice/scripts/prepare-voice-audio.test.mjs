import { spawnSync } from "node:child_process"
import { mkdtempSync, readFileSync, rmSync } from "node:fs"
import { createRequire } from "node:module"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { afterEach, describe, expect, it } from "vitest"

const require = createRequire(import.meta.url)
const script = fileURLToPath(
  new URL("./prepare-voice-audio.mjs", import.meta.url)
)
const sources = {
  "rawAudioProcessor.js": "@elevenlabs/client/worklets/rawAudioProcessor.js",
  "audioConcatProcessor.js":
    "@elevenlabs/client/worklets/audioConcatProcessor.js",
  "libsamplerate.worklet.js":
    "@alexanderolsen/libsamplerate-js/dist/libsamplerate.worklet.js",
}

let dir
afterEach(() => dir && rmSync(dir, { recursive: true, force: true }))

describe("prepare-voice-audio", () => {
  it("copies the installed SDK's audio processors into <publicDir>/voice-audio", () => {
    dir = mkdtempSync(join(tmpdir(), "voice-audio-"))
    const run = spawnSync(process.execPath, [script, "public"], {
      cwd: dir,
      encoding: "utf8",
    })
    expect(run.status, run.stderr).toBe(0)
    for (const [file, source] of Object.entries(sources))
      expect(readFileSync(join(dir, "public/voice-audio", file))).toEqual(
        readFileSync(require.resolve(source))
      )
  })

  it("refuses to run without a target folder", () => {
    const run = spawnSync(process.execPath, [script], { encoding: "utf8" })
    expect(run.status).toBe(1)
    expect(run.stderr).toContain("usage: prepare-voice-audio.mjs <publicDir>")
  })
})
