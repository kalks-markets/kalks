-- Kalks Circle (the social network) was removed from the platform on 2026-10-10. The gateway re-seeds its module
-- catalogue on every start, so the old "Kalks Circle" module row would stay in the owner grid: remove it (the
-- per-broker switches in tenant_features go with it, ON DELETE CASCADE) and drop its staff permissions from roles.
DELETE FROM feature_flags WHERE key = 'circle';

UPDATE roles
   SET permissions = ARRAY(SELECT p FROM unnest(permissions) AS p WHERE p NOT LIKE 'circle.%')
 WHERE EXISTS (SELECT 1 FROM unnest(permissions) AS p WHERE p LIKE 'circle.%');
