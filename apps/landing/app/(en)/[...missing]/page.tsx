import { notFound } from "next/navigation"

// Any other path is a 404, rendered per request like the page so its scripts carry the CSP nonce
// (a prerendered 404 has no nonce and its scripts would be blocked).
export default function Missing() {
  notFound()
}
