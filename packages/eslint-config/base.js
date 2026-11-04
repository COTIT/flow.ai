import js from "@eslint/js"
import eslintConfigPrettier from "eslint-config-prettier"
import { globalIgnores } from "eslint/config"
import turboPlugin from "eslint-plugin-turbo"
import globals from "globals"
import tseslint from "typescript-eslint"

/**
 * Shared ESLint rules for every package in the repo.
 *
 * @type {import("eslint").Linter.Config[]}
 */
export const config = [
  // .next-stack, dist-stack: the full-stack e2e builds (packages/e2e/STACK.md).
  globalIgnores(["dist/**", ".next/**", "dist-stack/**", ".next-stack/**", "coverage/**", "src/generated/**"]),
  // ESLint only picks up .js/.mjs/.cjs by default; claim the TS/JSX files too.
  { files: ["**/*.{ts,tsx,mts,cts,js,jsx,mjs,cjs}"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  eslintConfigPrettier,
  {
    languageOptions: {
      globals: globals.node,
    },
    plugins: {
      turbo: turboPlugin,
    },
    rules: {
      "turbo/no-undeclared-env-vars": "warn",
    },
  },
]
