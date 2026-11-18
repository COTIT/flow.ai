// shortcut: localhost default for dev; every deploy must set SITE_URL.
export const siteUrl = (path: string) =>
  new URL(path, process.env.SITE_URL ?? "http://localhost:3000").href

export const shopNoindex = () => process.env.NOINDEX === "1"
