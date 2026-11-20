import { createRequestConfig } from "@verzo/i18n/request"

export default createRequestConfig(
  "en",
  async (locale) => (await import(`../messages/${locale}.json`)).default
)
