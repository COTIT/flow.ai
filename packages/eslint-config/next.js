import { globalIgnores } from "eslint/config"
import pluginNext from "@next/eslint-plugin-next"
import pluginReactHooks from "eslint-plugin-react-hooks"
import globals from "globals"

import { config as baseConfig } from "./base.js"

/**
 * ESLint rules for the Next.js apps.
 *
 * @type {import("eslint").Linter.Config[]}
 */
export const nextJsConfig = [
  ...baseConfig,
  globalIgnores(["out/**", "build/**", "next-env.d.ts"]),
  // Third-party React Bits files, kept byte-for-byte (see components/rb/README.md).
  globalIgnores(["components/rb/**"]),
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.serviceworker,
      },
    },
  },
  {
    plugins: {
      "@next/next": pluginNext,
    },
    rules: {
      ...pluginNext.configs.recommended.rules,
      ...pluginNext.configs["core-web-vitals"].rules,
      // Images are API-served uploads and Satori og-images; next/image would change how they load.
      "@next/next/no-img-element": "off",
    },
  },
  pluginReactHooks.configs.flat.recommended,
]
