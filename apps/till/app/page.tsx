import { redirect } from "next/navigation"

/** The sale screen is home; its guard sends an unpaired or logged-out device to setup or login. */
export default function Page() {
  redirect("/sale")
}
