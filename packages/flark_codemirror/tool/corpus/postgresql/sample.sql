-- Inventory service schema for PostgreSQL.
/* Functions, triggers and reports;
   /* nested comments close in order */ still a comment */

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TYPE item_state AS ENUM ('draft', 'active', 'retired');

CREATE TABLE items (
  id bigserial PRIMARY KEY,
  sku text NOT NULL UNIQUE,
  "Display Name" varchar(200),
  price numeric(10, 2) CHECK (price >= 0),
  tags text[] DEFAULT '{}',
  attrs jsonb NOT NULL DEFAULT '{}'::jsonb,
  state item_state DEFAULT 'draft',
  created timestamptz NOT NULL DEFAULT now(),
  uid uuid DEFAULT gen_random_uuid()
);

CREATE INDEX items_attrs_idx ON items USING gin (attrs jsonb_path_ops);

CREATE OR REPLACE FUNCTION touch_item() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.created := now();
  IF NEW.price IS NULL THEN
    RAISE NOTICE 'price missing for %', NEW.sku;
  END IF;
  RETURN NEW;
END;
$$;

CREATE FUNCTION price_band(p numeric) RETURNS text AS $body$
  SELECT CASE
    WHEN p < 10 THEN 'low'
    WHEN p BETWEEN 10 AND 100 THEN 'mid'
    ELSE 'high'
  END;
$body$ LANGUAGE sql IMMUTABLE STRICT;

CREATE TRIGGER items_touch BEFORE UPDATE ON items
  FOR EACH ROW EXECUTE FUNCTION touch_item();

INSERT INTO items (sku, price, tags, attrs)
VALUES ('A-100', 9.99, ARRAY['red', 'small'], '{"weight": 2}'),
       ('B-200', 120, '{blue,large}', jsonb_build_object('weight', 40))
ON CONFLICT (sku) DO UPDATE SET price = EXCLUDED.price
RETURNING id, sku;

UPDATE items SET attrs = attrs || '{"sale": true}', state = 'active'
WHERE attrs ? 'weight' AND attrs->>'weight' IS NOT NULL;

SELECT sku, attrs->'weight' AS w, attrs #>> '{dims,0}' AS d0,
       tags[1] AS first_tag, price::int, '2024-05-01'::date + 7,
       E'tab\there', e'line\nbreak', 'back\slash stays', 'it''s'
FROM items
WHERE sku ~* '^a-' AND sku !~ 'x$' AND tags @> ARRAY['red']
  AND attrs <@ '{"weight": 2, "sale": true}' AND sku ILIKE 'b%';

WITH RECURSIVE parts(id, parent, depth) AS (
  SELECT id, NULL::bigint, 0 FROM items WHERE sku = 'A-100'
  UNION ALL
  SELECT i.id, p.id, p.depth + 1
  FROM items i JOIN parts p ON i.id = p.id + 1
  WHERE p.depth < 5
)
SELECT * FROM parts ORDER BY depth NULLS LAST;

SELECT state, count(*) FILTER (WHERE price > 50) AS pricey,
       avg(price) OVER w, percentile_cont(0.5) WITHIN GROUP (ORDER BY price),
       lag(price, 1) OVER (PARTITION BY state ORDER BY created
                           ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)
FROM items
GROUP BY state, price, created
WINDOW w AS (PARTITION BY state);

SELECT x, y FROM generate_series(1, 3) AS x,
  LATERAL (SELECT x * 2 AS y) AS doubled;

SELECT 1e3, .5, 5., 0x1F, X'1F', B'1010', b'01', n'national', N'x',
       _utf8'bytes', 3 # 5, 7 & 3, 2 ^ 10, @ -5, |/ 25.0, 10 % 3;

SELECT DATE '2024-01-01', TIME '10:00', TIMESTAMP '2024-01-01 10:00',
       INTERVAL '1 day', now() - interval '2 hours';

COPY items (sku, price) FROM stdin WITH (FORMAT csv, HEADER true);
\copy items TO 'items.csv' CSV
\d items

DO $$
DECLARE
  n integer := 0;
BEGIN
  FOR n IN 1..3 LOOP
    PERFORM pg_sleep(0.1);
  END LOOP;
END $$;

SELECT ? AS placeholder, $1::text AS param, coalesce(NULL, 'x');
DROP TABLE IF EXISTS items CASCADE;
