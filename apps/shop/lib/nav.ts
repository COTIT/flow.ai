/** The shop's main pages, in menu order; labels are `nav.<key>` messages. */
export const NAV = [
  { href: "/shop", key: "shop" },
  { href: "/stories", key: "stories" },
  { href: "/about", key: "about" },
  { href: "/faq", key: "faq" },
  { href: "/contact", key: "contact" },
] as const

/** A `mailto:` link to the shop's address with a ready subject line. */
export const mailto = (email: string, subject: string) =>
  `mailto:${email}?subject=${encodeURIComponent(subject)}`
