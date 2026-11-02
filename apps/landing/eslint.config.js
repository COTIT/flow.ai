import { nextJsConfig } from "@genesy/eslint-config/next-js"

/** @type {import("eslint").Linter.Config} */
export default [
  ...nextJsConfig,
  // Third-party React Bits files, kept byte-for-byte (see components/rb/README.md).
  { ignores: ["components/rb/**"] },
]
