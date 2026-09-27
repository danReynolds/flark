/*
 * Inventory.java: generics, annotations, text blocks and switches.
 */
package com.example.inventory;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import static java.util.stream.Collectors.toMap;

/** Marks an item that needs review. */
@Retention(RetentionPolicy.RUNTIME)
@interface Review {
  String reason() default "unspecified";
  int priority() default 1;
}

public final class Inventory<T extends Comparable<T>> implements Iterable<T> {
  private static final long MAX_ITEMS = 1_000_000L;
  private static final double RATE = 0.075d, SCALE = 1.5e-3, HALF = .5f;
  private static final int MASK = 0xFF_FF, BITS = 0b1010_1010;
  private final List<T> items = new ArrayList<>();
  private volatile boolean dirty;

  static {
    System.out.println("loaded");
  }

  public Inventory(List<? extends T> initial) {
    this.items.addAll(initial);
  }

  @Override
  public java.util.Iterator<T> iterator() {
    return items.iterator();
  }

  @SuppressWarnings("unchecked")
  public <R> List<R> map(Function<? super T, ? extends R> mapper) {
    List<R> result = new ArrayList<>(items.size());
    for (T item : items) {
      result.add(mapper.apply(item));
    }
    return result;
  }

  public synchronized void add(T item) throws IllegalStateException {
    if (items.size() >= MAX_ITEMS) {
      throw new IllegalStateException("full: " + items.size());
    } else if (item == null) {
      return;
    }
    items.add(item);
    dirty = true;
  }

  String describe(int code) {
    String label;
    switch (code) {
      case 0:
        label = "none";
        break;
      case 1:
      case 2:
        label = "few";
        break;
      default:
        label = code > 100 ? "many" : "some";
    }
    return switch (label) {
      case "none" -> "empty";
      case "few", "some" -> {
        yield label.toUpperCase();
      }
      default -> label;
    };
  }

  static String report(Map<String, Integer> counts) {
    String header = """
        Inventory report
          "quoted" and \"escaped\" lines
        """;
    StringBuilder out = new StringBuilder(header);
    counts.forEach((name, count) -> out.append(name)
        .append('=')
        .append(count)
        .append('\n'));
    char tab = '\t', quote = '\'';
    return out.append(tab).append(quote).toString();
  }

  enum Level {
    LOW("low"), HIGH("high");

    private final String name;

    Level(String name) {
      this.name = name;
    }
  }

  record Pair<A, B>(A first, B second) {}

  public static void main(String[] args) throws Exception {
    Inventory<String> inventory = new Inventory<>(List.of("b", "a"));
    outer:
    for (int i = 0; i < args.length; i++) {
      for (int j = i; j < args.length; j++) {
        if (args[j].isEmpty()) continue outer;
        if (j > 10) break outer;
      }
    }
    Runnable task = new Runnable() {
      @Override
      public void run() {
        System.out.println(inventory.map(String::length));
      }
    };
    try (var reader = new java.io.StringReader("text")) {
      task.run();
    } catch (RuntimeException | Error e) {
      e.printStackTrace();
    } finally {
      assert inventory != null : "no inventory";
    }
    Object value = args.length > 0 ? args[0] : Integer.valueOf(42);
    if (value instanceof String s && !s.isBlank()) {
      System.out.printf("%s%n", s);
    }
  }
}
