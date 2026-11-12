import type { Policy } from "./office-parity-scan.mjs"
export type CoverageRow = {
  area: string
  actions: number
  voice: number
  handoff: number
  officeOnly: number
}
export function areaOf(file: string): string
export function coverage(
  ops: Map<string, Set<string>>,
  operations: Policy
): CoverageRow[]
export function coverageMarkdown(
  rows: CoverageRow[],
  totals: { execute: number; withWords: number; withPicture: number }
): string
