/** Enough of a storefront variant to choose between options. */
type Optioned = { options: { type: string; value: string }[] }

/** Each option once, with its values in the order the variants first use them. */
export function optionTypes(variants: Optioned[]) {
  const types = new Map<string, string[]>()
  for (const { options } of variants)
    for (const { type, value } of options) {
      const values = types.get(type) ?? types.set(type, []).get(type)!
      if (!values.includes(value)) values.push(value)
    }
  return [...types].map(([type, values]) => ({ type, values }))
}

const valueOf = (v: Optioned, type: string) =>
  v.options.find((o) => o.type === type)?.value

/**
 * The variant with `type` set to `value` and the other choices of `current` kept; when that
 * combination isn't sold, the first variant with `value`, so a choice always lands on a real one.
 */
export function pick<V extends Optioned>(
  variants: V[],
  current: V | null,
  type: string,
  value: string
): V | null {
  const withValue = variants.filter((v) => valueOf(v, type) === value)
  const keeps = (v: V) =>
    current?.options.every(
      (o) => o.type === type || valueOf(v, o.type) === o.value
    ) ?? false
  return withValue.find(keeps) ?? withValue[0] ?? null
}
