import { config } from "@genesy/eslint-config/react-internal"

// Vendored React Bits files stay byte-for-byte, ignored as they were in Office (see src/components/rb).
/** @type {import("eslint").Linter.Config} */
export default [...config, { ignores: ["src/components/rb/**", "src/components/strands.tsx"] }]
