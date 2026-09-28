$users = $db->query('SELECT * FROM users')->fetchAll(PDO::FETCH_ASSOC);
foreach ($users as $user) {
    if ($user['active']) {
        printf("%s <%s>\n", $user['name'], $user['email']);
    }
}

function greet(string $name, ?string $greeting = null): string {
    $greeting ??= 'Hello';
    return "$greeting, {$name}!";
}

$handler = static fn($x) => $x * 2;
echo greet('World') . PHP_EOL;
