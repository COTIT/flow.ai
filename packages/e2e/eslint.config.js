import { config } from "@genesy/eslint-config/base"

/** @type {import("eslint").Linter.Config} */
export default [
  ...config,
  { ignores: ["playwright-report/**", "test-results/**", ".stack/**", "voice-probe/out/**"] },
  // The full-stack harness and the voice probe run outside turbo (STACK.md), so turbo's env hashing doesn't apply.
  {
    files: ["stack/**", "src/stack.ts", "voice-probe/**"],
    rules: { "turbo/no-undeclared-env-vars": "off" },
  },
]
