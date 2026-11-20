export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface MeasuredRange {
  /** Where the caret at `end` is, relative to the textarea's top left corner. */
  caret: Box;
  /** One box per line of the range from `start` to `end`; empty for a plain caret. */
  rects: Box[];
}

/** The styles that decide where the browser breaks and places text; copied onto the mirror. */
const COPIED = [
  "boxSizing",
  "width",
  "borderTopWidth",
  "borderRightWidth",
  "borderBottomWidth",
  "borderLeftWidth",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "fontFamily",
  "fontSize",
  "fontWeight",
  "fontStyle",
  "fontVariant",
  "letterSpacing",
  "wordSpacing",
  "lineHeight",
  "textTransform",
  "textIndent",
  "tabSize",
] as const;

/**
 * Where characters `start` to `end` of a textarea's text are drawn. A textarea gives no
 * coordinates for its text, so the same text is laid out in an invisible `div` with the same
 * styles and measured there. The textarea must not scroll (the editor grows it to fit).
 */
export function measureRange(
  textarea: HTMLTextAreaElement,
  start: number,
  end: number,
): MeasuredRange {
  const text = textarea.value;
  const from = Math.max(0, Math.min(start, end, text.length));
  const to = Math.max(0, Math.min(Math.max(start, end), text.length));
  const style = getComputedStyle(textarea);

  const mirror = document.createElement("div");
  mirror.setAttribute("aria-hidden", "true");
  mirror.style.position = "absolute";
  mirror.style.visibility = "hidden";
  mirror.style.top = "0";
  mirror.style.left = "-99999px";
  mirror.style.whiteSpace = "pre-wrap";
  mirror.style.overflowWrap = "break-word";
  for (const name of COPIED) mirror.style[name] = style[name];

  const marker = document.createElement("span");
  marker.textContent = from === to ? "​" : text.slice(from, to);
  mirror.append(document.createTextNode(text.slice(0, from)), marker, text.slice(to));
  document.body.append(mirror);
  try {
    const origin = mirror.getBoundingClientRect();
    const relative = (r: DOMRect): Box => ({
      left: r.left - origin.left,
      top: r.top - origin.top,
      width: r.width,
      height: r.height,
    });
    const rects = Array.from(marker.getClientRects()).map(relative);
    const first = rects[0] ?? relative(marker.getBoundingClientRect());
    const last = rects[rects.length - 1] ?? first;
    // The caret sits at the end of the range: the right edge of its last line.
    const caret: Box = {
      left: from === to ? first.left : last.left + last.width,
      top: from === to ? first.top : last.top,
      width: 0,
      height: from === to ? first.height : last.height,
    };
    return { caret, rects: from === to ? [] : rects };
  } finally {
    mirror.remove();
  }
}
