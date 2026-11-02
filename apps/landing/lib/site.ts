/** `mailto:` link of the one call to action. shortcut: placeholder address until the real one is set. */
export function contactHref(configured: string | undefined) {
  const email = configured?.trim() || "hello@genesy.app"
  return `mailto:${email}?subject=${encodeURIComponent("Genesy for my organisation")}`
}

type Env = Partial<Record<string, string>>

/** Where the footer sends people to shop or sign in; staging sets its own hosts. */
export function appLinks(env: Env) {
  return {
    shop: env.NEXT_PUBLIC_SHOP_URL || "https://shop.genesy.app",
    office: env.NEXT_PUBLIC_OFFICE_URL || "https://office.genesy.app",
    field: env.NEXT_PUBLIC_FIELD_URL || "https://field.genesy.app",
    till: env.NEXT_PUBLIC_TILL_URL || "https://till.genesy.app",
  }
}
