-- n / d rounded half away from zero to a whole number, exact (div truncates without a
-- result scale; plain round(n / d) rounds the quotient first and can misround near .5).
-- Every money calculation that divides goes through here (CLAUDE.md money rule).
CREATE FUNCTION money_round_div(n numeric, d numeric) RETURNS numeric
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN sign(n) * sign(d) * div(2 * abs(n) + abs(d), 2 * abs(d));
