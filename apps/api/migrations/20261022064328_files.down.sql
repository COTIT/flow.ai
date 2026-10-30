-- The bucket is never listed: objects under genesy/ outlive this table. Empty the prefix by
-- hand when rolling back for good.
DROP TABLE files;
