import { createRequestConfig } from "@genesy/i18n/request"

export default createRequestConfig(
  "sw",
  async (locale) => (await import(`../messages/${locale}.json`)).default
)
