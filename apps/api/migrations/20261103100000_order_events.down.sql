DROP TABLE order_events;
ALTER TABLE orders
    DROP COLUMN picked_at,
    DROP COLUMN confirmed_at;
