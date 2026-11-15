/**
 * Minimal text diff by common prefix and suffix (docs/ARCHITECTURE.md §3.2).
 *
 * Offsets are UTF-16 code units, the same unit as JavaScript strings, textarea selections and
 * the Yrs document (`OffsetKind::Utf16`). A cut never lands between the two halves of a
 * surrogate pair, so an emoji is always replaced as a whole.
 */
export interface TextDiff {
  /** Start of the change, in UTF-16 units. */
  index: number;
  /** How many units of the previous text are removed. */
  deleteCount: number;
  /** Text inserted at `index` after the removal. */
  insert: string;
}

const isHighSurrogate = (unit: number): boolean => unit >= 0xd800 && unit <= 0xdbff;
const isLowSurrogate = (unit: number): boolean => unit >= 0xdc00 && unit <= 0xdfff;

/** Returns the edit that turns `prev` into `next`, or `null` when they are equal. */
export function diffText(prev: string, next: string): TextDiff | null {
  if (prev === next) return null;

  const shortest = Math.min(prev.length, next.length);

  let prefix = 0;
  while (prefix < shortest && prev.charCodeAt(prefix) === next.charCodeAt(prefix)) prefix++;
  // The texts differ inside a pair, so the shared high half does not belong to the prefix.
  if (prefix > 0 && isHighSurrogate(prev.charCodeAt(prefix - 1))) prefix--;

  let suffix = 0;
  while (
    suffix < shortest - prefix &&
    prev.charCodeAt(prev.length - 1 - suffix) === next.charCodeAt(next.length - 1 - suffix)
  ) {
    suffix++;
  }
  // Likewise, a suffix may not start on the low half of a pair whose high half differs.
  if (suffix > 0 && isLowSurrogate(prev.charCodeAt(prev.length - suffix))) suffix--;

  return {
    index: prefix,
    deleteCount: prev.length - prefix - suffix,
    insert: next.slice(prefix, next.length - suffix),
  };
}

/** Applies a diff to `prev`. Used by tests and to map a caret across a remote edit. */
export function applyDiff(prev: string, diff: TextDiff): string {
  return prev.slice(0, diff.index) + diff.insert + prev.slice(diff.index + diff.deleteCount);
}

/** Moves a caret or selection offset across an edit made elsewhere in the text. */
export function shiftOffset(offset: number, diff: TextDiff): number {
  if (offset <= diff.index) return offset;
  if (offset >= diff.index + diff.deleteCount) {
    return offset + diff.insert.length - diff.deleteCount;
  }
  // The offset was inside the removed range: put it after the inserted text.
  return diff.index + diff.insert.length;
}
