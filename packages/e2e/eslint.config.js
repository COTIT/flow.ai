import { config } from "@genesy/eslint-config/base"

/** @type {import("eslint").Linter.Config} */
export default [
  ...config,
  { ignores: ["playwright-report/**", "test-results/**", ".stack/**"] },
  // The full-stack harness runs outside turbo (STACK.md), so turbo's env hashing doesn't apply.
  {
    files: ["stack/**", "src/stack.ts"],
    rules: { "turbo/no-undeclared-env-vars": "off" },
  },
]
