<?php

// Pricing parity oracle. Runs vimdesk-api's real item maths (Infrastructure\Values\Item,
// BuildItemsSummary, BuildItemsMargeCalculationSummary) on fixed cases, prints JSON.
// Needs a vimdesk-api checkout with vendor/ installed:
//   php -d display_errors=stderr tests/fixtures/pricing_oracle.php /path/to/apps/vimdesk-api > tests/fixtures/pricing_parity.json

require $argv[1].'/vendor/autoload.php';

use Infrastructure\Actions\BuildItemsMargeCalculationSummary;
use Infrastructure\Actions\BuildItemsSummary;
use Infrastructure\Values\Item;

// Each case: items as vimdesk-web sends them (numbers or strings), no product_id/relation_id.
$cases = [
    'three_at_9_99_vat21' => [['id' => 1, 'quantity' => 3, 'price' => 9.99, 'vat' => 21]],
    'three_at_9_99_vat21_discount10' => [['id' => 1, 'quantity' => 3, 'price' => 9.99, 'vat' => 21, 'productDiscount' => 10]],
    'three_lines_0_07_vat21' => [
        ['id' => 1, 'quantity' => 1, 'price' => 0.07, 'vat' => 21],
        ['id' => 2, 'quantity' => 1, 'price' => 0.07, 'vat' => 21],
        ['id' => 3, 'quantity' => 1, 'price' => 0.07, 'vat' => 21],
    ],
    'mixed_rates_0_9_21' => [
        ['id' => 1, 'quantity' => 2, 'price' => 12.5, 'vat' => 21],
        ['id' => 2, 'quantity' => 1, 'price' => 3.99, 'vat' => 9],
        ['id' => 3, 'quantity' => 4, 'price' => 1.25, 'vat' => 0],
    ],
    'string_inputs' => [['id' => 'a', 'quantity' => '2', 'price' => '9.9900', 'vat' => '21', 'productDiscount' => '5']],
    'negative_quantity_credit' => [['id' => 1, 'quantity' => -3, 'price' => 9.99, 'vat' => 21]],
    'negative_quantity_credit_discount' => [['id' => 1, 'quantity' => -3, 'price' => 9.99, 'vat' => 21, 'productDiscount' => 10]],
    'full_discount' => [['id' => 1, 'quantity' => 2, 'price' => 50, 'vat' => 21, 'productDiscount' => 100]],
    'zero_quantity' => [['id' => 1, 'quantity' => 0, 'price' => 19.95, 'vat' => 21]],
    'empty_values' => [['id' => 1, 'quantity' => '', 'price' => '', 'vat' => '']],
    'fractional_quantity' => [['id' => 1, 'quantity' => 1.5, 'price' => 3.33, 'vat' => 9]],
    'four_dp_price_from_incl' => [['id' => 1, 'quantity' => 1, 'price' => '8.2562', 'vat' => 21]],
    'half_cent_vat_tie' => [['id' => 1, 'quantity' => 1, 'price' => 0.5, 'vat' => 21]],
    'half_cent_line_tie' => [['id' => 1, 'quantity' => 3, 'price' => 0.335, 'vat' => 21]],
    'discount_12_5' => [['id' => 1, 'quantity' => 7, 'price' => 14.99, 'vat' => 21, 'productDiscount' => 12.5]],
    'many_small_discounted_lines' => array_map(
        fn ($i) => ['id' => $i, 'quantity' => 1, 'price' => 0.99, 'vat' => 21, 'productDiscount' => 7],
        range(1, 10)
    ),
    'vat_5_5_and_5' => [
        ['id' => 1, 'quantity' => 1, 'price' => 100, 'vat' => 5.5],
        ['id' => 2, 'quantity' => 1, 'price' => 100, 'vat' => 5],
    ],
    'large_amounts' => [['id' => 1, 'quantity' => 1000, 'price' => 123456.78, 'vat' => 21, 'productDiscount' => 3]],
    'only_zero_vat' => [['id' => 1, 'quantity' => 2, 'price' => 10, 'vat' => 0]],
    'margin_with_purchase_price' => [
        ['id' => 1, 'quantity' => 2, 'price' => 25, 'vat' => 21, 'purchase_price' => 15],
        ['id' => 2, 'quantity' => 1, 'price' => 10, 'vat' => 21, 'productDiscount' => 10, 'purchase_price' => '4.5'],
    ],
    'margin_zero_revenue' => [['id' => 1, 'quantity' => 1, 'price' => 0, 'vat' => 21, 'purchase_price' => 5]],
    'margin_skips_set_children' => [
        ['id' => 1, 'quantity' => 1, 'price' => 30, 'vat' => 21, 'purchase_price' => 10],
        ['id' => 2, 'quantity' => 1, 'price' => 30, 'vat' => 21, 'purchase_price' => 10, 'multi_set_item_id' => 1],
    ],
    'mixed_signs' => [
        ['id' => 1, 'quantity' => 2, 'price' => 19.99, 'vat' => 21],
        ['id' => 2, 'quantity' => -1, 'price' => 19.99, 'vat' => 21],
    ],
    'price_three_dp' => [['id' => 1, 'quantity' => 3, 'price' => 1.005, 'vat' => 21]],
];

// CalculateItemsRequest::validated(): empty vat/price/quantity become 0.
function to_item(array $raw): Item
{
    foreach (['vat', 'price', 'quantity'] as $k) {
        if (empty($raw[$k])) {
            $raw[$k] = 0;
        }
    }

    return new Item(
        id: $raw['id'],
        quantity: (float) $raw['quantity'],
        price: (float) $raw['price'],
        vat: (float) $raw['vat'],
        productDiscount: $raw['productDiscount'] ?? null,
        purchase_price: $raw['purchase_price'] ?? null,
        multi_set_item_id: $raw['multi_set_item_id'] ?? null,
    );
}

// Data::toArray needs the Laravel container; public props are the same fields.
function plain(mixed $v): mixed
{
    if (is_object($v)) {
        $v = get_object_vars($v);
    }

    return is_array($v) ? array_map('plain', $v) : $v;
}

$out = ['generator' => 'PHP '.PHP_VERSION.' vimdesk-api BuildItemsSummary + BuildItemsMargeCalculationSummary', 'cases' => []];
foreach ($cases as $name => $items) {
    $values = collect($items)->map(fn ($i) => to_item($i));
    $summary = (new BuildItemsSummary)->buildSummary($values, 2);
    $marge = (new BuildItemsMargeCalculationSummary)->buildSummary($values, 2);
    $out['cases'][] = [
        'name' => $name,
        'items' => $items,
        'summary' => json_decode(json_encode(plain($summary)), true),
        'marge' => json_decode(json_encode(plain($marge)), true),
    ];
}
echo json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES), "\n";
