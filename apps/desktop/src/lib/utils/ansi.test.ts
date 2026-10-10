import { describe, expect, it } from 'vitest';
import { parseAnsi, stripAnsi } from './ansi';

describe('parseAnsi', () => {
  it('returns plain text unchanged', () => {
    expect(parseAnsi('hello')).toEqual([{ text: 'hello', fg: null, bold: false, dim: false }]);
  });

  it('parses ESP-IDF log colors', () => {
    const segments = parseAnsi('\u001b[0;32mI (10) app: ready\u001b[0m');
    expect(segments).toEqual([{ text: 'I (10) app: ready', fg: 2, bold: false, dim: false }]);
  });

  it('handles bright colors, bold and resets', () => {
    const segments = parseAnsi('a\u001b[1;91mb\u001b[39mc\u001b[0md');
    expect(segments.map((s) => [s.text, s.fg, s.bold])).toEqual([
      ['a', null, false],
      ['b', 9, true],
      ['c', null, true],
      ['d', null, false],
    ]);
  });

  it('drops non-SGR escape sequences', () => {
    expect(stripAnsi('\u001b[2Kline\u001b[1A')).toBe('line');
  });
});
