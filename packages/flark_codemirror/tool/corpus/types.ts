import type { Config } from './config';
declare module 'shim' {
  export function load(path: string): Promise<Buffer>;
}
declare const VERSION: string;
namespace Geometry {
  export interface Point<T = number> {
    readonly x: T;
    y?: T;
    [key: string]: unknown;
    (scale: number): Point<T>;
    new (x: T, y: T): Point<T>;
  }
}
type Maybe<T> = T | null | undefined;
type Keys = keyof typeof defaults;
type Mapped<T> = { readonly [K in keyof T]?: T[K] };
type Cond<T> = T extends string ? 'text' : T extends Array<infer U> ? U : never;
type Tpl = `prefix-${string}`;
enum Direction { Up = 1, Down, Left = 'left', Right = Left }
const enum Flags { None = 0, A = 1 << 0 }
abstract class Shape implements Drawable, Serializable {
  private readonly id: number;
  protected static registry = new Map<string, Shape>();
  public abstract area(): number;
  constructor(public name: string, private scale = 1) {
    super();
  }
  @memoize()
  describe(this: Shape, verbose?: boolean): string {
    return `${this.name}: ${this.area().toFixed(2)}`;
  }
  get kind(): 'shape' { return 'shape'; }
}
function isString(value: unknown): value is string {
  return typeof value === 'string';
}
function first<T extends { length: number }>(items: T[], fallback?: T): T | undefined {
  return items[0] ?? fallback;
}
const handler = <T,>(event: CustomEvent<T>): void => {
  const detail = event.detail as T;
  const el = document.getElementById('x')!;
  let tuple: [string, number, ...boolean[]] = ['a', 1];
  const fn: (a: number, b?: string) => Promise<void> = async (a, b) => {};
};
export default function overload(a: string): string;
export default function overload(a: number): number;
let x: Array<Map<string, Set<number>>> = [];
const conf = { strict: true } satisfies Config;
for (const key in record) if (Object.hasOwn(record, key)) delete record[key];
class Box<T> { constructor(private value: T) {} unwrap = (): T => this.value; }
