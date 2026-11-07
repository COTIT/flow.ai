// Serve the installed SDK's matching audio processors from Office's own origin.
import { copyFile, mkdir } from "node:fs/promises"
import { createRequire } from "node:module"

const require = createRequire(import.meta.url)
const target = new URL("../public/voice-audio/", import.meta.url)
await mkdir(target, { recursive: true })
for (const file of ["rawAudioProcessor.js", "audioConcatProcessor.js"]) {
  await copyFile(
    require.resolve(`@elevenlabs/client/worklets/${file}`),
    new URL(file, target)
  )
}
await copyFile(
  require.resolve("@alexanderolsen/libsamplerate-js/dist/libsamplerate.worklet.js"),
  new URL("libsamplerate.worklet.js", target)
)
