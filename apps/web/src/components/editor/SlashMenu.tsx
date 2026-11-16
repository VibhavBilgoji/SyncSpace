import { cn } from "@/lib/cn";
import type { BlockType } from "@/types/generated";

export interface SlashItem {
  type: BlockType;
  label: string;
  hint: string;
}

/** Frame 10: the block types, in the order of the wireframe. */
export const SLASH_ITEMS: SlashItem[] = [
  { type: "HEADING_1", label: "Heading 1", hint: "Large section title" },
  { type: "HEADING_2", label: "Heading 2", hint: "Medium section title" },
  { type: "PARAGRAPH", label: "Paragraph", hint: "Plain text" },
  { type: "CODE", label: "Code", hint: "Monospaced block" },
  { type: "CHECKLIST", label: "Checklist", hint: "Item with a checkbox" },
  { type: "CALLOUT", label: "Callout", hint: "Highlighted note" },
];

/** The items whose label contains `query` (case-insensitive). */
export function filterSlashItems(query: string): SlashItem[] {
  const needle = query.trim().toLowerCase();
  return SLASH_ITEMS.filter((item) => item.label.toLowerCase().includes(needle));
}

/** Filterable menu under a block. Keyboard handling lives in the block (focus stays there). */
export function SlashMenu({
  items,
  active,
  onPick,
}: {
  items: SlashItem[];
  active: number;
  onPick: (item: SlashItem) => void;
}) {
  if (items.length === 0) {
    return (
      <div
        role="status"
        className="absolute left-0 z-10 mt-1 w-64 rounded-card border border-border bg-elevated p-3 text-ui-sm text-text-muted"
      >
        No matching block type
      </div>
    );
  }
  return (
    <ul
      role="listbox"
      aria-label="Block types"
      className="absolute left-0 z-10 mt-1 w-64 overflow-hidden rounded-card border border-border bg-elevated py-1"
    >
      {items.map((item, index) => (
        <li
          key={item.type}
          role="option"
          aria-selected={index === active}
          onMouseDown={(event) => {
            event.preventDefault(); // keep focus in the block
            onPick(item);
          }}
          className={cn(
            "flex cursor-pointer flex-col px-3 py-1.5",
            index === active ? "bg-hover" : "hover:bg-hover",
          )}
        >
          <span className="text-ui-base text-text">{item.label}</span>
          <span className="text-ui-sm text-text-muted">{item.hint}</span>
        </li>
      ))}
    </ul>
  );
}
