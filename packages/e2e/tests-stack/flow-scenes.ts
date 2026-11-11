// F2: a fresh owner workspace for the Flow scenes stack specs (the shared seed stays untouched).
import type { APIRequestContext } from "@playwright/test"

import { apiAs, expect, mailTo, unique, urls } from "../src/stack"

export async function freshOwner(request: APIRequestContext, tag: string) {
  const name = `e2e-flow-${tag}-${unique()}`
  const login = {
    app: `flow (${tag})`,
    role: "owner",
    login: `${name}@e2e.test`,
    secret: `E2e!${unique()}Aa1`,
    path: "/login",
  }
  const signup = await request.post(`${urls.api}/v1/create-workspace`, {
    data: {
      first_name: "Flow",
      last_name: "Owner",
      email: login.login,
      password: login.secret,
      workspace_name: name,
    },
  })
  expect(signup.status()).toBe(201)
  const link = (await mailTo(login.login)).match(
    /https?:\/\/\S+\/v1\/verify-email\/\S+/
  )?.[0]
  expect(link).toBeTruthy()
  await request.get(link!, { maxRedirects: 0 })
  return { login, api: await apiAs(request, login) }
}
