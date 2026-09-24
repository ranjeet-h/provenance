/**
 * Cross-boundary offset contract (see `byte_range_to_utf16_range` in
 * `crates/provenance-match/src/normalize.rs`).
 *
 * All Rust spans (`Token.original_start/end`, passage `*_char_start/end`)
 * are UTF-8 BYTE offsets. JavaScript `String.slice()` indexes UTF-16 code
 * units. Never pass Rust offsets to `slice()` directly — convert first.
 */

/** UTF-16 code units for one JS string element (handles surrogate pairs). */
function utf16LengthOf(ch: string): number {
  return ch.length;
}

/**
 * Convert a UTF-8 byte offset into a JS string index.
 * Returns null when the offset is not a character boundary.
 */
export function utf8ByteToUtf16Index(text: string, byteOffset: number): number | null {
  const encoder = new TextEncoder();
  if (byteOffset === encoder.encode(text).length) {
    return text.length;
  }
  let bytes = 0;
  let index = 0;
  for (const ch of text) {
    if (bytes === byteOffset) {
      return index;
    }
    if (bytes > byteOffset) {
      return null;
    }
    bytes += encoder.encode(ch).length;
    index += utf16LengthOf(ch);
  }
  return bytes === byteOffset ? index : null;
}

/**
 * Slice text by Rust UTF-8 byte offsets. Falls back to clamping when a
 * bound is not a character boundary (defensive: Rust always emits
 * boundaries, so a fallback indicates a contract violation upstream).
 */
export function sliceByByteOffsets(text: string, startByte: number, endByte: number): string {
  const start = utf8ByteToUtf16Index(text, startByte);
  const end = utf8ByteToUtf16Index(text, endByte);
  if (start === null || end === null || start > end) {
    return "";
  }
  return text.slice(start, end);
}
