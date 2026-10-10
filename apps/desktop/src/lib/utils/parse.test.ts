import { describe, expect, it } from 'vitest';
import { checkOffset, checkRegion, parseNumber } from './parse';

describe('parseNumber', () => {
  it.each([
    ['0x1000', 0x1000],
    ['0X10_000', 0x10000],
    ['4096', 4096],
    [' 64K ', 65536],
    ['4MiB', 4 * 1024 * 1024],
    ['2mb', 2 * 1024 * 1024],
    ['0xFFFFFFFF', 0xffffffff],
  ])('parses %s', (input, expected) => {
    expect(parseNumber(input)).toEqual({ ok: true, value: expected });
  });

  it.each(['', '0x', '0xZZ', '-1', '1.5', 'abc', '0x100000000', '5000M', 'k'])('rejects %j', (input) => {
    expect(parseNumber(input).ok).toBe(false);
  });
});

describe('checkRegion', () => {
  it('accepts aligned regions inside the flash', () => {
    const r = checkRegion('0x9000', '0x6000', { alignOffset: true, alignLength: true, flashSize: 0x400000 });
    expect(r.valid).toBe(true);
  });

  it('requires sector alignment when asked', () => {
    expect(checkRegion('0x9001', '0x1000', { alignOffset: true }).offset.ok).toBe(false);
    expect(checkRegion('0x9000', '0x1001', { alignLength: true }).length.ok).toBe(false);
    expect(checkRegion('0x9001', '0x1001').valid).toBe(true);
  });

  it('rejects zero length and regions beyond the flash', () => {
    expect(checkRegion('0', '0').length.ok).toBe(false);
    const beyond = checkRegion('0x3FF000', '0x2000', { flashSize: 0x400000 });
    expect(beyond.valid).toBe(false);
    expect(beyond.region).toMatch(/beyond/);
  });
});

describe('checkOffset', () => {
  it('validates alignment and flash bounds', () => {
    expect(checkOffset('0x10000', { align: true }).ok).toBe(true);
    expect(checkOffset('0x10010', { align: true }).ok).toBe(false);
    expect(checkOffset('0x400000', { flashSize: 0x400000 }).ok).toBe(false);
  });
});
