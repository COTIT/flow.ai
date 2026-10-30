<?php

// Price correction parity oracle. Runs vimdesk-api's real GetRoundedPrice and MoneyHelper on
// fixed cases, with the tail of ProductPriceTrait::buildPrice (a model method, so its three
// lines are repeated here), and prints JSON.
// Needs a vimdesk-api checkout with vendor/ installed:
//   php -d display_errors=stderr tests/fixtures/price_correction_oracle.php /path/to/apps/vimdesk-api > tests/fixtures/price_correction_parity.json

require $argv[1].'/vendor/autoload.php';

use Domain\ERP\Product\V1\Actions\GetRoundedPrice;
use Infrastructure\Helpers\MoneyHelper;

// [amount excl VAT, vat %, round_on, remove_amount]; null = column null.
$cases = [
    'nines_on_5ct' => ['9.99', '21', '0.05', '0.01'],
    'whole_euro_minus_cent' => ['10', '21', '1', '0.01'],
    'tie_rounds_down' => ['12.5', '0', '1', null],
    'half_step_tie' => ['3.25', '0', '0.5', null],
    'round_up_nearer' => ['3.30', '0', '0.5', null],
    'no_round_on' => ['10', '21', null, '0.05'],
    'zero_round_on' => ['10', '21', '0', '0.05'],
    'no_remove' => ['7.77', '9', '0.1', null],
    'vat_9' => ['4.99', '9', '0.1', '0.01'],
    'large' => ['123456.789', '21', '5', '0.05'],
    'zero_amount' => ['0', '21', '1', '0.01'],
    'negative_round_on' => ['10.3', '0', '-1', null],
    'tiny_step' => ['1.23456789', '21', '0.00000001', null],
    'odd_vat' => ['19.95', '5.5', '0.25', '0.02'],
];

$out = [];
foreach ($cases as $name => [$amount, $vat, $roundOn, $remove]) {
    $mult = MoneyHelper::convertVatFromIntToDecimals(vat: floatval($vat));
    $rounded = (new GetRoundedPrice)->__invoke(value: $amount, roundOn: $roundOn, vat: $vat);
    $price = (string) ($rounded - ($remove / $mult));
    $out[] = [
        'name' => $name,
        'amount' => $amount,
        'vat' => $vat,
        'round_on' => $roundOn,
        'remove_amount' => $remove,
        'price' => MoneyHelper::formatDecimals($price, 4),
        'price_incl' => MoneyHelper::formatDecimals(floatval($price) * $mult),
    ];
}
echo json_encode($out, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES), "\n";
