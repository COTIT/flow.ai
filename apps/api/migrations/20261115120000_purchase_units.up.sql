-- Product-specific purchase units (Flow ERP masterplan §5 UNITS): an input unit a supplier
-- sells in, with an exact ratio to the product's base unit. Meaning (code, category, ratio,
-- product) never changes once written; only `active` toggles, bumping `version`.
CREATE TABLE purchase_units (
    id                        uuid          PRIMARY KEY DEFAULT uuidv7(),
    workspace_id              uuid          NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    product_id                uuid          NOT NULL,
    code                      text          NOT NULL CHECK (char_length(code) BETWEEN 1 AND 50),
    measurement_category      text          NOT NULL CHECK (measurement_category IN
                                            ('mass', 'length', 'area', 'volume', 'count', 'packaging')),
    base_units_per_input_unit numeric(20,8) NOT NULL CHECK (base_units_per_input_unit > 0),
    active                    boolean       NOT NULL DEFAULT true,
    version                   integer       NOT NULL DEFAULT 1 CHECK (version >= 1),
    -- Permanent create identity: same client_ref + same request replays, other content is 409.
    client_ref                uuid          NOT NULL,
    request_hash              bytea         NOT NULL,
    -- Last PATCH identity, replayed while it is the latest change.
    last_client_ref           uuid,
    last_request_hash         bytea,
    created_by                uuid          NOT NULL REFERENCES users (id),
    created_at                timestamptz   NOT NULL DEFAULT now(),
    updated_at                timestamptz   NOT NULL DEFAULT now(),
    CONSTRAINT purchase_units_workspace_id_id_key UNIQUE (workspace_id, id),
    CONSTRAINT purchase_units_client_ref_key UNIQUE (workspace_id, client_ref),
    CONSTRAINT purchase_units_product_fk FOREIGN KEY (workspace_id, product_id)
        REFERENCES products (workspace_id, id)
);
CREATE UNIQUE INDEX purchase_units_code_key
    ON purchase_units (workspace_id, product_id, lower(code));

-- Ratio meaning is immutable: purchases freeze it, so a later edit would rewrite history.
CREATE FUNCTION purchase_units_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.workspace_id <> OLD.workspace_id OR NEW.product_id <> OLD.product_id
       OR NEW.code <> OLD.code OR NEW.measurement_category <> OLD.measurement_category
       OR NEW.base_units_per_input_unit <> OLD.base_units_per_input_unit
       OR NEW.client_ref <> OLD.client_ref OR NEW.request_hash <> OLD.request_hash THEN
        RAISE EXCEPTION 'purchase unit meaning is immutable' USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER purchase_units_immutable BEFORE UPDATE ON purchase_units
    FOR EACH ROW EXECUTE FUNCTION purchase_units_immutable();
