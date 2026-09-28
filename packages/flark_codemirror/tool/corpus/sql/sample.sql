-- A small lending library: schema, data and reports.
/* Block comments may span lines
   and /* nest */ like this. */

CREATE TABLE members (
  id INTEGER NOT NULL,
  name VARCHAR(80) NOT NULL,
  email VARCHAR(120),
  joined DATE,
  balance DECIMAL(8, 2) DEFAULT 0.00,
  active BOOLEAN DEFAULT TRUE,
  PRIMARY KEY (id)
);

CREATE TABLE books (id INT, title TEXT, isbn CHAR(13), price NUMERIC(6,2),
                    published YEAR, pages SMALLINT);

CREATE TABLE loans (
  book_id INT REFERENCES books (id),
  member_id INT REFERENCES members (id),
  taken TIMESTAMP NOT NULL,
  due TIMESTAMP,
  returned TIMESTAMP NULL
);

ALTER TABLE members ADD COLUMN phone VARCHAR(20);
DROP TABLE IF EXISTS old_loans;

INSERT INTO members (id, name, email, joined)
VALUES (1, 'Ada Lovelace', 'ada@example.org', DATE '2024-01-15'),
       (2, 'Grace O''Brien', "grace@example.org", DATE "2024-02-01"),
       (3, 'Alan \'Al\' Turing', NULL, NULL);

INSERT INTO books VALUES (10, 'SICP', '9780262510875', 49.95, 1996, 657);
INSERT INTO books VALUES (11, 'The Art of Computer Programming', NULL, 1.5e2,
  1968, 672);

UPDATE members SET balance = balance - 2.50, active = FALSE
WHERE id = ? AND joined < TIMESTAMP '2024-03-01 00:00:00';

DELETE FROM loans WHERE returned IS NOT NULL AND taken < TIME '12:00';

SELECT DISTINCT m.name, b.title, l.taken
FROM loans l
JOIN members m ON m.id = l.member_id
JOIN books b ON b.id = l.book_id
WHERE l.returned IS NULL
  AND (l.due < CURRENT_DATE OR l.due IS UNKNOWN)
ORDER BY l.taken DESC, m.name ASC
LIMIT 20;

-- Aggregates with grouping and a filter on the groups.
SELECT member_id, COUNT(*) AS loans, SUM(b.price) total
FROM loans
JOIN books b ON b.id = book_id
GROUP BY member_id
HAVING COUNT(*) >= 3 AND SUM(b.price) <> 0
ORDER BY total DESC;

WITH overdue AS (
  SELECT member_id, COUNT(*) AS n
  FROM loans
  WHERE due < CURRENT_TIMESTAMP
  GROUP BY member_id
), ranked AS (
  SELECT member_id, n,
         RANK() OVER (ORDER BY n DESC) AS place,
         ROW_NUMBER() OVER (PARTITION BY n ORDER BY member_id) AS row_no
  FROM overdue
)
SELECT * FROM ranked WHERE place <= 10;

SELECT name FROM members WHERE name LIKE 'A%'
UNION
SELECT title FROM books WHERE title LIKE '%Programming%';

SELECT id, price * 1.07 AS gross, pages / 2 half, pages % 7 rest,
       -price, +price, price||'EUR', 0x1F, x'0A0B', X'ff', b'0101', 0b11
FROM books
WHERE id BETWEEN 10 AND 20 AND id IN (10, 11, 12) AND NOT id = 13;

SELECT m.id, schema_name.members.name, .5 AS half, 1. AS one
FROM members m WHERE m.id != 4 AND m.balance >= -1.0e-2;

-- Brackets and parentheses left open across lines.
SELECT coalesce(
  email,
  'none'
), [1, 2,
  3]
FROM members;

SELECT COUNT(DISTINCT member_id) FROM loans WHERE book_id IN (
  SELECT id FROM books WHERE price > (SELECT AVG(price)
                                      FROM books)
);

BEGIN;
UPDATE books SET price = price * 0.9 WHERE published < 1990;
INSERT INTO loans (book_id, member_id, taken) VALUES (?, ?, ?);
COMMIT;

SELECT 'unterminated string runs to the end of the line
SELECT "and a double one
SELECT 'escaped quote \' inside', "two "" quotes";
