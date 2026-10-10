/**
 * Client-side parsing of addresses and lengths for immediate feedback. The
 * backend validates every value again before touching hardware.
 */

export const SECTOR_SIZE = 0x1000;
export const MAX_FLASH = 0x1000_0000;
const U32_MAX = 0xffff_ffff;

export type ParseResult = { ok: true; value: number } | { ok: false; error: string };

export function parseNumber(input: string): ParseResult {
  const cleaned = input.trim().replace(/_/g, '');
  if (cleaned === '') return { ok: false, error: 'A value is required.' };
  const invalid: ParseResult = { ok: false, error: `"${input.trim()}" is not a valid number.` };

  const hex = /^0x([0-9a-f]+)$/i.exec(cleaned);
  if (hex) {
    const value = parseInt(hex[1]!, 16);
    if (hex[1]!.length > 8 || value > U32_MAX) return { ok: false, error: 'The value is too large.' };
    return { ok: true, value };
  }
  const sized = /^(\d+)\s*(k|kb|kib|m|mb|mib)?$/i.exec(cleaned);
  if (!sized) return invalid;
  const unit = (sized[2] ?? '').toLowerCase();
  const multiplier = unit.startsWith('k') ? 1024 : unit.startsWith('m') ? 1024 * 1024 : 1;
  const value = Number(sized[1]) * multiplier;
  if (!Number.isSafeInteger(value) || value > U32_MAX) return { ok: false, error: 'The value is too large.' };
  return { ok: true, value };
}

export interface RegionRules {
  /** Require 4 KiB alignment of the offset. */
  alignOffset?: boolean;
  /** Require the length to be a multiple of 4 KiB. */
  alignLength?: boolean;
  /** Flash capacity in bytes, if known. */
  flashSize?: number | null;
}

export interface RegionCheck {
  offset: ParseResult;
  length: ParseResult;
  /** Error concerning the combination of offset and length. */
  region: string | null;
  valid: boolean;
}

export function checkRegion(offsetInput: string, lengthInput: string, rules: RegionRules = {}): RegionCheck {
  let offset = parseNumber(offsetInput);
  let length = parseNumber(lengthInput);
  if (offset.ok && rules.alignOffset && offset.value % SECTOR_SIZE !== 0) {
    offset = { ok: false, error: 'Must be aligned to 4 KiB (0x1000).' };
  }
  if (length.ok && length.value === 0) {
    length = { ok: false, error: 'Must be greater than zero.' };
  } else if (length.ok && rules.alignLength && length.value % SECTOR_SIZE !== 0) {
    length = { ok: false, error: 'Must be a multiple of 4 KiB (0x1000).' };
  }
  let region: string | null = null;
  if (offset.ok && length.ok) {
    const end = offset.value + length.value;
    if (end > MAX_FLASH) {
      region = 'The region extends beyond the maximum flash size of 256 MiB.';
    } else if (rules.flashSize != null && end > rules.flashSize) {
      region = 'The region extends beyond the end of the flash.';
    }
  }
  return { offset, length, region, valid: offset.ok && length.ok && region === null };
}

export function checkOffset(input: string, rules: { align?: boolean; flashSize?: number | null } = {}): ParseResult {
  const result = parseNumber(input);
  if (!result.ok) return result;
  if (rules.align && result.value % SECTOR_SIZE !== 0) {
    return { ok: false, error: 'Must be aligned to 4 KiB (0x1000).' };
  }
  if (rules.flashSize != null && result.value >= rules.flashSize) {
    return { ok: false, error: 'Lies beyond the end of the flash.' };
  }
  return result;
}
