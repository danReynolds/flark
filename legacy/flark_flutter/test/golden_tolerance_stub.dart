/// No-op on platforms without `dart:io` (web): the VM comparator's API is absent
/// there, and web golden comparison is not the flaky path this smooths.
void installTolerantGoldenComparator() {}
