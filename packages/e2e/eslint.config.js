import { config } from "@genesy/eslint-config/base"

/** @type {import("eslint").Linter.Config} */
export default [
  ...config,
  { ignores: ["playwright-report/**", "test-results/**"] },
]
