<?php
declare(strict_types=1);

namespace App\Billing;

use App\Contracts\{Invoice, Payable};
use function sprintf;

#[Attribute(Attribute::TARGET_CLASS)]
final class Ledger implements Payable
{
    public const VERSION = '2.1';
    private static int $count = 0;

    public function __construct(
        private readonly string $owner,
        protected array $entries = [],
        public ?Invoice $invoice = null,
    ) {
        self::$count++;
    }

    public function add(string $label, float $amount = 0.0): static
    {
        $this->entries[] = ['label' => $label, 'amount' => $amount];
        return $this;
    }

    public function total(): float
    {
        return array_sum(array_map(fn(array $e): float => $e['amount'], $this->entries));
    }

    public function describe(): string
    {
        $name = strtoupper($this->owner);
        $first = $this->entries[0]['label'] ?? 'none';
        # A hash comment
        // A line comment
        /* A block
           comment */
        return "Ledger for {$name} with {$this->total()} and $first, count ${count}";
    }

    public function status(int $code): string
    {
        return match (true) {
            $code >= 500 => 'error',
            $code >= 400, $code === 0 => 'client',
            default => 'ok',
        };
    }

    public function report(): string
    {
        $rows = implode("\n", array_column($this->entries, 'label'));
        return <<<EOT
        Report for {$this->owner}
        Rows: $rows
        EOT;
    }

    public function raw(): string
    {
        return <<<'NOW'
        No $interpolation {here}
        NOW;
    }
}

enum Status: string
{
    case Paid = 'paid';
    case Due = 'due';
}

$ledger = (new Ledger(owner: 'ada'))->add('coffee', 3.5)?->add('tea', 2);
if ($ledger instanceof Payable && !empty($ledger->entries)) {
    foreach ($ledger->entries as $key => $entry) {
        echo sprintf("%s: %.2f\n", $entry['label'], $entry['amount']);
    }
} elseif (isset($other)) {
    unset($other);
} else {
    throw new \RuntimeException('empty ledger');
}
$values = [1, 2, 3, ...$more];
$doubled = array_filter($values, function ($v) use ($ledger) { return $v % 2 === 0; });
$hex = 0x1F; $oct = 0o17; $bin = 0b101; $float = 1.5e3;
?>
<p>Total: <?= number_format($ledger->total(), 2) ?></p>
