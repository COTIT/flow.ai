// Serve the installed SDK's matching audio processors from the host app's own origin.
// Usage (from an app folder): node ../../packages/voice/scripts/prepare-voice-audio.mjs public
import { copyFile, mkdir } from "node:fs/promises"
import { createRequire } from "node:module"
import { resolve } from "node:path"

const [publicDir] = process.argv.slice(2)
if (!publicDir) {
  console.error("usage: prepare-voice-audio.mjs <publicDir>")
  process.exit(1)
}

const require = createRequire(import.meta.url)
const target = resolve(publicDir, "voice-audio")
await mkdir(target, { recursive: true })
for (const file of ["rawAudioProcessor.js", "audioConcatProcessor.js"]) {
  await copyFile(
    require.resolve(`@elevenlabs/client/worklets/${file}`),
    resolve(target, file)
  )
}
await copyFile(
  require.resolve("@alexanderolsen/libsamplerate-js/dist/libsamplerate.worklet.js"),
  resolve(target, "libsamplerate.worklet.js")
)
// PDF embeds the same fonts as the app, including non-ASCII business labels.
const fonts = resolve(publicDir, "report-fonts")
await mkdir(fonts, { recursive: true })
for (const file of ["Poppins-Regular.ttf", "Poppins-SemiBold.ttf"])
  await copyFile(new URL(`../../ui/src/fonts/${file}`, import.meta.url), resolve(fonts, file))
