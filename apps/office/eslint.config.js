import { nextJsConfig } from "@genesy/eslint-config/next-js"

/** @type {import("eslint").Linter.Config} */
export default [...nextJsConfig, { ignores: ["public/voice-audio/**", "components/voice/strands.tsx"] }]
