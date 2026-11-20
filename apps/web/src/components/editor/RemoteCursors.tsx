"use client";

import { useLayoutEffect, useState, type RefObject } from "react";
import { cursorColorOf } from "@/components/ui/Avatar";
import type { RemoteCursor } from "@/lib/doc/useDocumentSession";
import { measureRange, type Box } from "@/lib/textMeasure";

interface Placed {
  key: string;
  name: string;
  color: string;
  caret: Box;
  rects: Box[];
}

/**
 * Other people's carets and selections in one block: a coloured line with the name above it, and
 * a tinted box behind the selected text. The colour is the one of the person's avatar. Drawn over
 * the textarea, never in the way of the mouse. Positions are measured again when the text, the
 * cursors or the width of the block change.
 */
export function RemoteCursors({
  textarea,
  content,
  type,
  cursors,
}: {
  textarea: RefObject<HTMLTextAreaElement | null>;
  content: string;
  type: string;
  cursors: RemoteCursor[];
}) {
  const [placed, setPlaced] = useState<Placed[]>([]);
  const signature = cursors.map((c) => `${c.clientId}:${c.anchor}:${c.head}:${c.name}`).join("|");

  useLayoutEffect(() => {
    const element = textarea.current;
    if (!element) return;
    const place = () => {
      const next = cursors.map((c) => {
        const length = element.value.length;
        const anchor = Math.min(c.anchor, length);
        const head = Math.min(c.head, length);
        const measured = measureRange(element, Math.min(anchor, head), Math.max(anchor, head));
        // A backwards selection keeps the caret at its start: the head.
        const at = head < anchor ? measureRange(element, head, head).caret : measured.caret;
        const shift = (b: Box): Box => ({
          ...b,
          left: b.left + element.offsetLeft,
          top: b.top + element.offsetTop,
        });
        return {
          key: String(c.clientId),
          name: c.name,
          color: cursorColorOf(c.name),
          caret: shift(at),
          rects: measured.rects.map(shift),
        };
      });
      setPlaced(next);
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
    // `signature` stands for `cursors`: a new array with the same positions needs no new layout.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature, content, type, textarea]);

  if (placed.length === 0) return null;
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0"
      data-testid="remote-cursors"
    >
      {placed.map((p) => (
        <div key={p.key} data-remote-cursor={p.name}>
          {p.rects.map((r, i) => (
            <span
              key={i}
              data-cursor-selection=""
              className="absolute rounded-sm"
              style={{
                left: r.left,
                top: r.top,
                width: r.width,
                height: r.height,
                background: `color-mix(in srgb, ${p.color} 25%, transparent)`,
              }}
            />
          ))}
          <span
            data-cursor-caret=""
            className="absolute w-0.5"
            style={{
              left: p.caret.left - 1,
              top: p.caret.top,
              height: p.caret.height,
              background: p.color,
            }}
          />
          <span
            className="absolute -translate-y-full rounded-sm px-1.5 py-px text-micro font-semibold whitespace-nowrap text-canvas"
            style={{ left: p.caret.left - 1, top: p.caret.top, background: p.color }}
          >
            {p.name}
          </span>
        </div>
      ))}
    </div>
  );
}
