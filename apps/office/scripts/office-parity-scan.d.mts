export type Policy = Record<
  string,
  {
    method: string
    path: string
    disposition: string
    reason: string
    risk: string
  }
>
export const policy: Policy
export function opOf(name: string): string | undefined
export function officeScreenOps(): {
  ops: Map<string, Set<string>>
  unmatched: { file: string; route: string }[]
}
