export interface HighlightPart {
  text: string;
  match: boolean;
}

/** Split `text` into parts, marking case-insensitive occurrences of `term`. */
export function highlight(text: string, term: string): HighlightPart[] {
  if (!term) return [{ text, match: false }];
  const lower = text.toLowerCase();
  const needle = term.toLowerCase();
  const parts: HighlightPart[] = [];
  let index = 0;
  while (index < text.length) {
    const found = lower.indexOf(needle, index);
    if (found === -1) break;
    if (found > index) parts.push({ text: text.slice(index, found), match: false });
    parts.push({ text: text.slice(found, found + needle.length), match: true });
    index = found + needle.length;
  }
  if (index < text.length) parts.push({ text: text.slice(index), match: false });
  return parts.length ? parts : [{ text, match: false }];
}
