ALTER TABLE farm_persons DROP CONSTRAINT farm_persons_consent_stamp,
    DROP COLUMN passport_profile_consent_by,
    DROP COLUMN passport_profile_consent_at,
    DROP COLUMN passport_profile_consent;
DROP TABLE farm_passbook_entries;
DROP FUNCTION farm_passbook_append_only();
