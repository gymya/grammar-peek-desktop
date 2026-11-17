// API offsets and JavaScript string indices both use UTF-16 code units.
export function correctText(text, matches) {
  let cursor = 0;
  let result = "";
  let applied = 0;
  let unresolved = 0;
  const changes = [];
  const edits = matches.slice().sort((a, b) => a.offset - b.offset || b.length - a.length);
  for (const match of edits) {
    const { offset, length } = match;
    const replacement = match.replacements?.[0]?.value;
    if (!Number.isInteger(offset) || !Number.isInteger(length) || offset < cursor ||
        offset < 0 || length < 0 || offset + length > text.length ||
        typeof replacement !== "string") {
      unresolved++;
      continue;
    }
    if (text.slice(offset, offset + length) === replacement) continue;
    result += text.slice(cursor, offset);
    changes.push({ start: result.length, length: replacement.length, original: text.slice(offset, offset + length) });
    result += replacement;
    cursor = offset + length;
    applied++;
  }
  return { text: result + text.slice(cursor), applied, unresolved, changes };
}
