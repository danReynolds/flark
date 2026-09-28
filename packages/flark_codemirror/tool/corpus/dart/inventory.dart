// A small library: classes, records, patterns and every string form.
library inventory;

import 'dart:async';
import 'dart:math' as math show max, min;
import 'package:meta/meta.dart' deferred as meta;

part 'inventory.g.dart';

/// A stock item. Doc comments use three slashes.
@immutable
class Item implements Comparable<Item> {
  const Item(this.name, {required this.count, this.price = .5});

  final String name;
  final int count;
  final double price;

  static const int maxCount = 1_000_000;
  static const mask = 0xFF_FF;
  static const tiny = 1.5e-3;

  @override
  int compareTo(Item other) => name.compareTo(other.name);

  @override
  String toString() => 'Item($name, count: ${count * 2}, price: $price)';
}

/* Block comments /* nest */ in Dart. */
enum Level {
  low('L'),
  high('H');

  const Level(this.code);
  final String code;
}

mixin Audited on Object {
  final List<String> log = [];
  void record(String entry) => log.add(entry);
}

extension ItemTotals on Iterable<Item> {
  int get total => fold(0, (sum, item) => sum + item.count);
}

sealed class Event {}

final class Added extends Event {
  Added(this.item);
  final Item item;
}

typedef Handler = FutureOr<void> Function(Event event);

class Inventory with Audited {
  final Map<String, Item> _items = {};
  late final StreamController<Event> _events = StreamController.broadcast();

  Stream<Event> get events => _events.stream;

  Future<void> add(Item item) async {
    if (_items.containsKey(item.name)) {
      throw StateError('duplicate "${item.name}"');
    }
    _items[item.name] = item;
    record('added ${item.name}');
    _events.add(Added(item));
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }

  Stream<int> counts() async* {
    for (final item in _items.values) {
      yield item.count;
    }
  }

  Iterable<String> names() sync* {
    yield* _items.keys;
  }

  String describe(Object? value) => switch (value) {
    null => 'nothing',
    int n when n > 100 => 'many',
    (String a, int b) => 'pair $a $b',
    Item(:final name) => 'item $name',
    _ => 'other',
  };

  String report() {
    final buffer = StringBuffer()
      ..writeln('''
Inventory report
  items: ${_items.length}
  total: ${_items.values.total}
''')
      ..write(r'raw \n $notInterpolated')
      ..write(r"""raw triple
still raw ${nope}""")
      ..write("tab\t 'single' \"double\" \$dollar");
    return buffer.toString();
  }
}

void main(List<String> args) {
  final inventory = Inventory();
  var count = args.isEmpty ? 0 : int.tryParse(args.first) ?? -1;
  final (low, high) = (math.min(count, 1), math.max(count, 10));
  final levels = [
    for (final level in Level.values)
      if (level != Level.low) level.code,
  ];
  switch (count) {
    case 0:
      print('none');
      break;
    case > 0 && < 10:
      print('few');
    default:
      print('many: $count');
  }
  try {
    inventory.add(Item('bolt', count: count));
  } on StateError catch (e) {
    print(e);
  } finally {
    count += 1;
  }
  inventory.events.listen((event) {
    if (event case Added(:final item)) print(item);
  });
  print('$low $high ${levels.join(', ')} ${inventory.names().toList()}');
}
