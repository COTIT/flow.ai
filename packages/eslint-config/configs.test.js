import assert from "node:assert/strict"
import { test } from "node:test"
import { ESLint } from "eslint"

import { config as base } from "./base.js"
import { nextJsConfig } from "./next.js"
import { config as reactInternal } from "./react-internal.js"

// A component with a typed prop, an unused variable and a conditional hook.
const badTsx = `
import { useState } from "react"

type Props = { on: boolean }

export function Bad({ on }: Props) {
  const unused = 1
  if (on) useState(0)
  return <div />
}
`

/** Rule ids reported as errors (not warnings) for `code` at `filePath`. */
async function errors(config, filePath, code = badTsx) {
  const eslint = new ESLint({
    overrideConfigFile: true,
    overrideConfig: config,
    cwd: import.meta.dirname,
  })
  const [result] = await eslint.lintText(code, { filePath, warnIgnored: true })
  return result.messages.filter((m) => m.severity === 2).map((m) => m.ruleId)
}

const configs = { base, "next-js": nextJsConfig, "react-internal": reactInternal }

for (const [name, config] of Object.entries(configs)) {
  test(`${name}: lints .ts files and fails on errors`, async () => {
    const code = "const unused: number = 1\nexport {}\n"
    assert.deepEqual(await errors(config, "src/a.ts", code), [
      "@typescript-eslint/no-unused-vars",
    ])
  })

  test(`${name}: lints .mjs files`, async () => {
    const code = "const unused = process.env.X\nexport {}\n"
    assert.deepEqual(await errors(config, "scripts/a.mjs", code), [
      "@typescript-eslint/no-unused-vars",
    ])
  })

  test(`${name}: skips build output and generated code`, async () => {
    for (const path of ["dist/a.ts", ".next/a.js", "src/generated/a.ts"]) {
      assert.deepEqual(await errors(config, path), [], path)
    }
  })
}

for (const [name, config] of Object.entries({
  "next-js": nextJsConfig,
  "react-internal": reactInternal,
})) {
  test(`${name}: lints .tsx with hook rules`, async () => {
    assert.deepEqual((await errors(config, "app/bad.tsx")).sort(), [
      "@typescript-eslint/no-unused-vars",
      "react-hooks/rules-of-hooks",
    ])
  })
}
