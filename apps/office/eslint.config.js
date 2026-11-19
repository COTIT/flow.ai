import { nextJsConfig } from "@verzo/eslint-config/next-js"

/** @type {import("eslint").Linter.Config} */
export default [...nextJsConfig, { ignores: ["public/voice-audio/**"] }]
