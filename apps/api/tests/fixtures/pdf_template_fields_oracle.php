<?php

// available-fields oracle. Loads vimdesk-api's real PdfTemplateFields (custom fields stubbed
// empty: genesy has none; Laravel's Arr::dot/get reimplemented below, no vendor/ needed) and
// writes {fields, variables} per domain, key order kept, to <out>/<domain>.json:
//   php tests/fixtures/pdf_template_fields_oracle.php /path/to/apps/vimdesk-api src/modules/pdf_templates/fields

namespace Illuminate\Support {
    class Arr
    {
        public static function dot($array, $prepend = '')
        {
            $out = [];
            foreach ($array as $key => $value) {
                if (is_array($value) && ! empty($value)) {
                    $out = array_merge($out, static::dot($value, $prepend.$key.'.'));
                } else {
                    $out[$prepend.$key] = $value;
                }
            }

            return $out;
        }

        public static function get($array, $key, $default = null)
        {
            if ($key === null) {
                return $array;
            }
            if (array_key_exists($key, $array)) {
                return $array[$key];
            }
            foreach (explode('.', $key) as $segment) {
                if (! is_array($array) || ! array_key_exists($segment, $array)) {
                    return $default;
                }
                $array = $array[$segment];
            }

            return $array;
        }
    }

    class EmptyCollection
    {
        public function toArray()
        {
            return [];
        }
    }
}

namespace {
$src = file_get_contents($argv[1].'/src/Domain/WorkspaceSetting/PdfTemplate/V1/Helpers/PdfTemplateFields.php');
// No database here: the custom field query becomes an empty collection.
$src = preg_replace(
    '/CustomField::query\(\).*?->mapWithKeys\(function \(CustomField \$customField\) \{.*?\}\);/s',
    'new \\Illuminate\\Support\\EmptyCollection();',
    $src,
    1,
    $count,
);
$src = str_replace('declare(strict_types=1);', '', $src);
if ($count !== 1) {
    fwrite(STDERR, "custom field query not found\n");
    exit(1);
}
eval('?>'.$src);

use Domain\WorkspaceSetting\PdfTemplate\V1\Helpers\PdfTemplateFields;

foreach (['invoice', 'estimate', 'order', 'purchase_order', 'collective_invoice', 'work_order', 'cash_register', 'relation'] as $domain) {
    $data = [
        'fields' => PdfTemplateFields::getPdfTemplateFields($domain),
        'variables' => PdfTemplateFields::getTemplateModelVariables($domain),
    ];
    file_put_contents("{$argv[2]}/{$domain}.json", json_encode($data, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)."\n");
}
}
