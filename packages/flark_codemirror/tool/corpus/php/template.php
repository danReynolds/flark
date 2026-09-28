<!DOCTYPE html>
<html>
<head>
  <title><?= htmlspecialchars($title) ?></title>
  <style>
    .row { color: #333; }
  </style>
</head>
<body>
  <?php if (count($items) > 0): ?>
    <ul class="items">
      <?php foreach ($items as $i => $item): ?>
        <li data-index="<?= $i ?>" class="<?php echo $i % 2 ? 'odd' : 'even'; ?>">
          <?= $item['name'] ?>
        </li>
      <?php endforeach; ?>
    </ul>
  <?php else: ?>
    <p>No items.</p>
  <?php endif; ?>
  <script>
    const count = <?= json_encode(count($items)) ?>;
    console.log("items", count);
  </script>
</body>
</html>
