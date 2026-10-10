/** A run of text with uniform styling. */
export interface AnsiSegment {
  text: string;
  /** Palette index 0–15 for the foreground, or null for the default. */
  fg: number | null;
  bold: boolean;
  dim: boolean;
}

const ESC = '\u001b';

/**
 * Split a line into styled segments, interpreting SGR (`ESC[...m`) codes
 * for the 16-color palette and bold/dim. Other escape sequences are removed.
 */
export function parseAnsi(line: string): AnsiSegment[] {
  if (!line.includes(ESC)) return [{ text: line, fg: null, bold: false, dim: false }];
  const segments: AnsiSegment[] = [];
  let fg: number | null = null;
  let bold = false;
  let dim = false;
  let text = '';
  const push = () => {
    if (text) segments.push({ text, fg, bold, dim });
    text = '';
  };

  let i = 0;
  while (i < line.length) {
    const ch = line[i]!;
    if (ch === ESC && line[i + 1] === '[') {
      let j = i + 2;
      while (j < line.length && !/[@-~]/.test(line[j]!)) j++;
      const final = line[j];
      const params = line.slice(i + 2, j);
      i = j + 1;
      if (final !== 'm') continue;
      push();
      const codes = params === '' ? [0] : params.split(';').map((p) => (p === '' ? 0 : Number(p)));
      for (const code of codes) {
        if (code === 0) {
          fg = null;
          bold = false;
          dim = false;
        } else if (code === 1) bold = true;
        else if (code === 2) dim = true;
        else if (code === 22) {
          bold = false;
          dim = false;
        } else if (code >= 30 && code <= 37) fg = code - 30;
        else if (code >= 90 && code <= 97) fg = code - 90 + 8;
        else if (code === 39) fg = null;
      }
    } else if (ch === ESC) {
      i += 2;
    } else {
      text += ch;
      i++;
    }
  }
  push();
  return segments.length ? segments : [{ text: '', fg: null, bold: false, dim: false }];
}

export function stripAnsi(line: string): string {
  return parseAnsi(line)
    .map((s) => s.text)
    .join('');
}
