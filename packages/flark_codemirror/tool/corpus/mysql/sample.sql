# Order tracking schema for MySQL 8.
-- A comment needs the space after the dashes;
--this is two minus operators and a word.
/* Block comments /* nest */ too. */

CREATE DATABASE IF NOT EXISTS `shop` DEFAULT CHARACTER SET utf8mb4;
USE `shop`;

CREATE TABLE `customers` (
  `id` INT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(100) NOT NULL,
  `e``mail` VARCHAR(255) DEFAULT NULL,
  `created_at` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `flags` BIT(8) DEFAULT b'00000001',
  `score` DOUBLE DEFAULT 0,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uniq_email` (`e``mail`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE orders (
  id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  customer_id INT UNSIGNED NOT NULL,
  total DECIMAL(10,2) NOT NULL DEFAULT 0.00,
  status ENUM('new', 'paid', 'shipped') DEFAULT 'new',
  note TEXT,
  FOREIGN KEY (customer_id) REFERENCES customers (id) ON DELETE CASCADE
);

INSERT INTO customers (name, `e``mail`) VALUES
  ('O\'Neil', "oneil@example.com"),
  ('Zoë', 'zoe@example.com'),
  (N'Łukasz', _utf8mb4'lukasz@example.com');

INSERT IGNORE INTO orders (customer_id, total) VALUES (1, 19.99), (2, .5), (3, 5.);
REPLACE INTO orders SET id = 7, customer_id = 1, total = 1e2;
INSERT INTO orders (customer_id, total) VALUES (1, 10)
  ON DUPLICATE KEY UPDATE total = total + VALUES(total);

SET @total := 0, @@session.sql_mode = 'STRICT_TRANS_TABLES';
SET @'quoted var' = 1, @"double" = 2, @`tick` = 3, @@global.max_connections = 200;
SELECT @total, @@version, @@local.time_zone;

SELECT c.name, COUNT(o.id) AS orders, SUM(o.total) AS spent,
       GROUP_CONCAT(DISTINCT o.status ORDER BY o.status SEPARATOR ', ') AS seen,
       IF(SUM(o.total) > 100, 'gold', 'plain') AS tier
FROM `customers` AS c
LEFT JOIN orders o ON o.customer_id = c.id
WHERE c.created_at >= DATE '2024-01-01' AND c.name REGEXP '^[A-Z]'
GROUP BY c.id WITH ROLLUP
HAVING spent > 0
ORDER BY spent DESC
LIMIT 10 OFFSET 5;

SELECT id, total, ROW_NUMBER() OVER (PARTITION BY customer_id ORDER BY total DESC) rn,
       total DIV 3, total MOD 3, total XOR 1, total <=> NULL, ~id, id << 2
FROM orders WHERE total BETWEEN 1 AND 100 AND status IN ('paid', 'shipped');

WITH recent AS (
  SELECT * FROM orders WHERE id > ?
)
SELECT * FROM recent;

SELECT 0x1F, X'1F', x'0a', 0b101, B'11', 1.5e-3, TRUE, FALSE, NULL, \N;

DELIMITER $$
CREATE PROCEDURE add_order(IN cid INT, IN amount DECIMAL(10,2))
BEGIN
  DECLARE n INT DEFAULT 0;
  IF amount <= 0 THEN
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'amount must be positive';
  END IF;
  WHILE n < 3 DO
    SET n = n + 1;
  END WHILE;
  INSERT INTO orders (customer_id, total) VALUES (cid, amount);
END $$
DELIMITER ;

CALL add_order(1, 25.00);
UPDATE orders SET status = 'shipped' WHERE id = 1 LIMIT 1;
DELETE FROM orders WHERE total = 0;
ALTER TABLE orders ADD INDEX idx_status (status), DROP COLUMN note;
SHOW TABLES;
EXPLAIN SELECT * FROM orders WHERE customer_id = 1\G
source backup.sql
status
TRUNCATE TABLE orders;
DROP TABLE IF EXISTS orders, customers;
