import type { MockReply } from "../../src/api-mock"

/** One farm role of the signed-in user, as GET /v1/farm/me returns it. */
export const role = (
  role: "farmer" | "field_officer" | "ops" | "admin",
  village: { id: string; name: string } | null = { id: "v1", name: "Ilundo" }
) => ({
  id: `m-${role}`,
  workspace_id: "w1",
  workspace_name: "ruaha-demo",
  display_name: "Ruaha demo",
  role,
  village_id: village?.id ?? null,
  village_name: village?.name ?? null,
  person_id: role === "farmer" ? "p1" : null,
})

/** GET /v1/farm/me reply. */
export const me = ({
  name = "Salima Officer",
  email = "salima@ruaha.test" as string | null,
  phone = null as string | null,
  locale = "en" as "en" | "sw" | null,
  mustChange = false,
  memberships = [role("field_officer")],
} = {}): MockReply => ({
  body: {
    data: {
      user: { id: "u1", name, email, phone },
      locale,
      must_change_password: mustChange,
      memberships,
    },
  },
})

export const villages: MockReply = {
  body: {
    data: [{ id: "v1", code: "ILUNDO", name: "Ilundo", latitude: null, longitude: null, primary_language: "sw" }],
    meta: { current_page: 1, from: 1, to: 1, last_page: 1, per_page: 100, total: 1 },
  },
}

export const staffLogin: MockReply = {
  body: { data: { access_token: "e2e-token", token_type: "Bearer", workspace_ids: ["w1"] } },
}

export const farmLogin = (mustChange: boolean): MockReply => ({
  body: {
    data: {
      access_token: "e2e-token",
      token_type: "Bearer",
      must_change_password: mustChange,
      memberships: [{ workspace_id: "w1", role: "farmer", village_id: "v1" }],
    },
  },
})
