ALTER TABLE variant_option_values
    ADD COLUMN display_value text CHECK (char_length(display_value) BETWEEN 1 AND 255),
    ADD COLUMN sort_order integer;
