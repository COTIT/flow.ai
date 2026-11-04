-- Offline till sales booked for a product that went off sale (disabled or archived) after
-- they were rung up: counted, never refused, like price mismatches. Additive.
ALTER TABLE till_sales
    ADD COLUMN unsellable_lines integer NOT NULL DEFAULT 0
        CONSTRAINT till_sales_unsellable_lines_check CHECK (unsellable_lines >= 0);
