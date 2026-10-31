import type { Meta } from '@genesy/api-client/model'

/** Every page of a paged API list, in order (100 a page, the API's maximum). */
export async function allPages<T>(
  read: (params: { page: number; page_size: number }) => Promise<{ data: T[]; meta: Meta }>,
): Promise<T[]> {
  const rows: T[] = []
  for (let page = 1, last = 1; page <= last; page++) {
    const res = await read({ page, page_size: 100 })
    rows.push(...res.data)
    last = res.meta.last_page
  }
  return rows
}
