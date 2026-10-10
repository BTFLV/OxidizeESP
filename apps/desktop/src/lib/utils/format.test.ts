import { describe, expect, it } from 'vitest';
import { basename, dirname, formatAddress, formatDuration, formatSize, joinPath } from './format';

describe('format', () => {
  it('formats addresses and sizes', () => {
    expect(formatAddress(0x8000)).toBe('0x00008000');
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(4096)).toBe('4 KiB');
    expect(formatSize(1536 * 1024)).toBe('1.5 MiB');
  });

  it('formats durations', () => {
    expect(formatDuration(250)).toBe('250 ms');
    expect(formatDuration(12_340)).toBe('12.3 s');
    expect(formatDuration(125_000)).toBe('2 min 5 s');
  });

  it('handles Windows and POSIX paths', () => {
    expect(basename('C:\\fw\\app.bin')).toBe('app.bin');
    expect(basename('/tmp/app.elf')).toBe('app.elf');
    expect(dirname('/tmp/app.elf')).toBe('/tmp');
    expect(joinPath('C:\\backups', 'a.bin')).toBe('C:\\backups\\a.bin');
    expect(joinPath('/home/user/', 'a.bin')).toBe('/home/user/a.bin');
  });
});
