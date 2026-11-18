import assert from "node:assert/strict"
import { test } from "node:test"

import { cut, htmlToText, references, transform } from "./afm-catalogue.mjs"

const page = {
  nestedCategories: {
    categories: [
      { id: "toys", displayName: "Toys" },
      { id: "fig", displayName: "3D Figurines" },
      { id: "art", displayName: "Artwork" },
    ],
  },
  items: [
    {
      urlId: "hyena",
      title: '"Handcrafted Hyena"',
      fullUrl: "/store/p/hyena",
      categoryIds: ["fig", "toys"],
      excerpt:
        "<p>Carved by <strong>Benard</strong> &amp; friends.</p><p>Soap&#8209;stone&nbsp;finish.</p>",
      items: [{ assetUrl: "https://images.example/a.PNG" }],
      structuredContent: {
        variantOptionOrdering: [],
        variants: [{ optionValues: [], sku: "H1", price: 2500 }],
      },
    },
    {
      urlId: "prints",
      title: "Prints",
      fullUrl: "/store/p/prints",
      categoryIds: ["art"],
      excerpt: null,
      items: null,
      structuredContent: {
        variantOptionOrdering: ["Print"],
        variants: [
          {
            optionValues: [{ optionName: "Print", value: "The Mary" }],
            sku: "P1",
            price: 1499,
          },
          {
            optionValues: [{ optionName: "Print", value: "Light" }],
            sku: "P2",
            price: 1499,
          },
        ],
      },
    },
  ],
}

test("a product keeps its first category, its cents and its photos", () => {
  const [hyena, prints] = transform(page)
  assert.deepEqual(hyena, {
    slug: "hyena",
    title: "Handcrafted Hyena",
    category: "Toys & games",
    short_description: "Carved by Benard & friends.",
    description: "Carved by Benard & friends.\n\nSoap‑stone finish.",
    options: [],
    variants: [{ values: [], sku: "H1", cents: 2500 }],
    images: ["https://images.example/a.PNG?format=1000w"],
    source_url: "https://www.africanfarmersmarketonline.com/store/p/hyena",
  })
  assert.equal(prints.category, "Art")
  assert.equal(prints.description, "")
  assert.deepEqual(prints.images, [])
  assert.deepEqual(
    prints.variants.map((v) => [v.values, v.cents]),
    [
      [["The Mary"], 1499],
      [["Light"], 1499],
    ]
  )
})

test("an unmapped category stops the import", () => {
  const odd = structuredClone(page)
  odd.nestedCategories.categories[2].displayName = "Jewellery"
  assert.throws(() => transform(odd), /no shop category for "Jewellery"/)
})

test("text is cut at a sentence, else a word, never mid-word", () => {
  assert.equal(cut("Short.", 10), "Short.")
  assert.equal(cut("One two three. Four five six seven.", 20), "One two three.")
  assert.equal(cut("alpha beta gamma delta", 12), "alpha beta…")
})

test("markup and scripts never survive", () => {
  assert.equal(htmlToText('<p>a<script>x</script></p><img src="y">'), "ax")
})

test("two options keep both values per variant, in the store's option order", () => {
  const two = structuredClone(page)
  two.items[1].structuredContent = {
    variantOptionOrdering: ["Basket colour", "Spice"],
    variants: [
      {
        optionValues: [
          { optionName: "Spice", value: "Cumin" },
          { optionName: "Basket colour", value: "Lamu Indigo" },
        ],
        sku: "G1",
        price: 7499,
      },
    ],
  }
  const [, gift] = transform(two)
  assert.deepEqual(gift.options, ["Basket colour", "Spice"])
  assert.deepEqual(gift.variants, [
    { values: ["Lamu Indigo", "Cumin"], sku: "G1", cents: 7499 },
  ])
})

test("a product in AFM's unlisted category takes its slug's shop category", () => {
  const hidden = structuredClone(page)
  hidden.items[0].categoryIds = ["unlisted"]
  hidden.items[0].urlId = "holiday-gift-basket-the-tea-tradition"
  assert.equal(transform(hidden)[0].category, "Gifts")
  hidden.items[0].urlId = "something-new"
  assert.throws(() => transform(hidden), /no shop category for "undefined"/)
})

test("references stay with their slug; new slugs get the next numbers", () => {
  const got = references(
    [{ slug: "a" }, { slug: "new-b" }, { slug: "c" }, { slug: "new-a" }],
    [
      { slug: "c", reference: "AFM-001" },
      { slug: "a", reference: "AFM-002" },
      { slug: "gone", reference: "AFM-003" },
    ]
  )
  assert.deepEqual(
    got.map((p) => [p.slug, p.reference]),
    [
      ["a", "AFM-002"],
      ["new-b", "AFM-005"],
      ["c", "AFM-001"],
      ["new-a", "AFM-004"],
    ]
  )
})
