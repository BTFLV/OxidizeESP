import { describe, expect, it } from 'vitest';
import { RingBuffer } from './ringbuffer';

describe('RingBuffer', () => {
  it('keeps only the newest items', () => {
    const b = new RingBuffer<number>(3);
    b.pushAll([1, 2, 3, 4, 5]);
    expect(b.toArray()).toEqual([3, 4, 5]);
    expect(b.length).toBe(3);
    expect(b.dropped).toBe(2);
    expect(b.at(0)).toBe(3);
    expect(b.at(3)).toBeUndefined();
  });

  it('resizes keeping the newest items', () => {
    const b = new RingBuffer<number>(5);
    b.pushAll([1, 2, 3, 4, 5]);
    expect(b.resized(2).toArray()).toEqual([4, 5]);
    expect(b.resized(10).toArray()).toEqual([1, 2, 3, 4, 5]);
  });

  it('clears', () => {
    const b = new RingBuffer<number>(2);
    b.pushAll([1, 2, 3]);
    b.clear();
    expect(b.length).toBe(0);
    expect(b.dropped).toBe(0);
  });

  it('rejects invalid capacities', () => {
    expect(() => new RingBuffer(0)).toThrow();
  });
});
