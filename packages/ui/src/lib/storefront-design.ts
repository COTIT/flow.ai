const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value)
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const integer = (value: unknown, min: number, max: number): value is number =>
  Number.isSafeInteger(value) && Number(value) >= min && Number(value) <= max
const text = (value: unknown, max: number): value is string =>
  typeof value === "string" && value.trim().length > 0 && [...value].length <= max

function safeProduct(value: unknown): boolean {
  if (!record(value)) return false
  return (
    typeof value.id === "string" &&
    uuid.test(value.id) &&
    text(value.name, 10000) &&
    (value.short_description == null ||
      typeof value.short_description === "string") &&
    integer(value.price_gross, 0, Number.MAX_SAFE_INTEGER) &&
    typeof value.currency === "string" &&
    /^[A-Z]{3}$/.test(value.currency) &&
    integer(value.currency_decimals, 0, 20) &&
    [null, "in_stock", "low", "sold_out"].includes(
      value.stock_status as string | null
    ) &&
    Array.isArray(value.variants) &&
    value.variants.every(
      (variant) =>
        record(variant) &&
        typeof variant.is_default === "boolean" &&
        integer(variant.price_gross, 0, Number.MAX_SAFE_INTEGER)
    ) &&
    Array.isArray(value.images) &&
    value.images.every(
      (image) =>
        record(image) &&
        typeof image.url === "string" &&
        /^https?:\/\//.test(image.url)
    )
  )
}

/** Renderer compatibility preflight; Rust owns complete document validation. */
export function canRenderStorefrontPage(
  document: unknown,
  collections: unknown
): boolean {
  if (
    !record(document) ||
    document.schema_version !== 1 ||
    document.page !== "home" ||
    !["forest", "neutral"].includes(String(document.theme)) ||
    !Array.isArray(document.sections) ||
    document.sections.length < 1 ||
    document.sections.length > 12 ||
    !Array.isArray(collections)
  )
    return false

  const ids = new Set<string>()
  const grids = new Set<string>()
  let heroes = 0
  for (const section of document.sections) {
    if (
      !record(section) ||
      typeof section.id !== "string" ||
      !/^[a-zA-Z0-9_-]{1,64}$/.test(section.id) ||
      ids.has(section.id) ||
      !record(section.props) ||
      !text(section.props.heading, 160)
    )
      return false
    ids.add(section.id)
    const props = section.props
    if (section.type === "hero") {
      if (
        ++heroes > 1 ||
        !["centered", "split"].includes(String(props.layout ?? "split")) ||
        (props.body !== undefined &&
          (typeof props.body !== "string" || [...props.body].length > 1000))
      )
        return false
    } else if (section.type === "product_grid") {
      const columns = props.columns ?? { mobile: 1, tablet: 2, desktop: 3 }
      if (
        (props.source ?? "catalog") !== "catalog" ||
        !integer(props.limit ?? 6, 1, 12) ||
        !record(columns) ||
        !integer(columns.mobile, 1, 2) ||
        !integer(columns.tablet, 1, 3) ||
        !integer(columns.desktop, 1, 4)
      )
        return false
      grids.add(section.id)
    } else return false
  }
  if (grids.size > 4 || collections.length !== grids.size) return false
  for (const collection of collections) {
    if (
      !record(collection) ||
      typeof collection.section_id !== "string" ||
      !grids.delete(collection.section_id) ||
      !Array.isArray(collection.products) ||
      collection.products.length > 12 ||
      !collection.products.every(safeProduct)
    )
      return false
  }
  return grids.size === 0
}
