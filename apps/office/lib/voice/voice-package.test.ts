import { readdirSync, readFileSync } from "node:fs"
import { expect, it } from "vitest"

// Server code may use the pure @genesy/voice/lib contracts, never its client components.
const office = new URL("../../", import.meta.url)
const server = ["lib/voice", "app/api/voice"].flatMap((root) =>
  readdirSync(new URL(root, office), { recursive: true, encoding: "utf8" })
    .filter((f) => /\.tsx?$/.test(f))
    .map((f) => `${root}/${f}`)
)

it.each(server)("%s imports no @genesy/voice components", (file) => {
  expect(readFileSync(new URL(file, office), "utf8")).not.toMatch(/["']@genesy\/voice\/components/)
})
