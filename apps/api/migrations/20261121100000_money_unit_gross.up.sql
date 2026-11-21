-- One unit at `unit` minor units excl VAT incl `vat` %, in minor units at `dp` decimals: a
-- one-line price_lines total (unit + VAT rounded half away from zero). Shop, Till and checkout
-- show it. NULL past bigint, or where the engine's rust_decimal range ended (unit × vat %).
CREATE FUNCTION money_unit_gross(unit bigint, vat numeric, dp int) RETURNS bigint
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN (SELECT CASE WHEN abs(unit * vat) <= 79228162514264337593543950335 * 10::numeric ^ dp
                             AND g BETWEEN -9223372036854775808 AND 9223372036854775807
                        THEN g::bigint END
            FROM (SELECT unit + round(unit * vat * 0.01) AS g) x);
