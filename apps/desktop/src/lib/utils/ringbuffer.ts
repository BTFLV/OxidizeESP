/** Fixed-capacity buffer that discards the oldest items when full. */
export class RingBuffer<T> {
  #items: (T | undefined)[];
  #start = 0;
  #length = 0;
  #dropped = 0;

  constructor(readonly capacity: number) {
    if (!Number.isInteger(capacity) || capacity <= 0) throw new Error('capacity must be a positive integer');
    this.#items = new Array<T | undefined>(capacity);
  }

  get length(): number {
    return this.#length;
  }

  /** Number of items discarded since creation or the last clear. */
  get dropped(): number {
    return this.#dropped;
  }

  push(item: T): void {
    const index = (this.#start + this.#length) % this.capacity;
    this.#items[index] = item;
    if (this.#length < this.capacity) {
      this.#length++;
    } else {
      this.#start = (this.#start + 1) % this.capacity;
      this.#dropped++;
    }
  }

  pushAll(items: Iterable<T>): void {
    for (const item of items) this.push(item);
  }

  at(index: number): T | undefined {
    if (index < 0 || index >= this.#length) return undefined;
    return this.#items[(this.#start + index) % this.capacity];
  }

  toArray(): T[] {
    const out: T[] = new Array(this.#length);
    for (let i = 0; i < this.#length; i++) out[i] = this.#items[(this.#start + i) % this.capacity] as T;
    return out;
  }

  clear(): void {
    this.#items = new Array<T | undefined>(this.capacity);
    this.#start = 0;
    this.#length = 0;
    this.#dropped = 0;
  }

  /** Return a new buffer with a different capacity, keeping the newest items. */
  resized(capacity: number): RingBuffer<T> {
    const next = new RingBuffer<T>(capacity);
    const items = this.toArray();
    next.pushAll(items.slice(Math.max(0, items.length - capacity)));
    return next;
  }
}
