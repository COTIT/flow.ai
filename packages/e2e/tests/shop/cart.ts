import type { Page } from "@playwright/test"

import { PRODUCT, VARIANT } from "../../mock/fixtures.mjs"

/** Starts the browser with `quantity` honey 500 g jars in the cart (`genesy.cart`, once). */
export async function seedCart(page: Page, quantity: number) {
  const cart = {
    state: {
      lines: [
        {
          productId: PRODUCT.honey,
          variantId: VARIANT.honey500,
          quantity,
          name: "Ilundo Wild Forest Honey",
          variantLabel: "Size: 500 g",
          priceGross: 2499,
          currency: "USD",
          decimals: 2,
        },
      ],
    },
    version: 0,
  }
  // Only when absent, so a cart the app empties stays empty on the next page load.
  await page.addInitScript((v) => {
    if (!localStorage.getItem("genesy.cart"))
      localStorage.setItem("genesy.cart", v)
  }, JSON.stringify(cart))
}
