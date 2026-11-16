"use client";

import { useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { cn } from "@/lib/cn";
import { diffText, shiftOffset } from "@/lib/textDiff";
import type { Block, BlockType } from "@/types/generated";
import { SlashMenu, filterSlashItems, type SlashItem } from "./SlashMenu";

export interface BlockActions {
  insertText(blockId: string, index: number, chunk: string): void;
  deleteText(blockId: string, index: number, length: number): void;
  /** Returns the new block's id so the caller can focus it. */
  createBlockAfter(afterId: string | null, type: BlockType, content: string): string | null;
  deleteBlock(blockId: string): void;
  setType(blockId: string, type: BlockType): void;
  setProps(blockId: string, props: Record<string, unknown>): void;
}

const textClass: Record<BlockType, string> = {
  PARAGRAPH: "text-body-md",
  HEADING_1: "text-display-xl",
  HEADING_2: "text-heading-lg",
  CODE: "font-mono text-mono-code bg-surface rounded-control px-3 py-2",
  CHECKLIST: "text-body-md",
  CALLOUT: "text-body-md bg-elevated border-l-4 border-indigo rounded-control px-3 py-2",
};

interface Props {
  block: Block;
  previousId: string | null;
  readOnly: boolean;
  /** True for exactly one block: the one that should hold the caret after a structural edit. */
  focusRequested: boolean;
  onFocused: () => void;
  onFocusBlock: (id: string) => void;
  actions: BlockActions;
}

/** One block: an auto-growing textarea whose edits are diffed into the document operations. */
export function BlockRow({
  block,
  previousId,
  readOnly,
  focusRequested,
  onFocused,
  onFocusBlock,
  actions,
}: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const shown = useRef(block.content);
  const [menu, setMenu] = useState<{ query: string; active: number } | null>(null);

  // Remote changes: show the new text and keep the caret where the user was typing.
  useLayoutEffect(() => {
    const textarea = ref.current;
    if (!textarea) return;
    if (textarea.value !== block.content) {
      const diff = diffText(shown.current, block.content);
      const start = textarea.selectionStart;
      const end = textarea.selectionEnd;
      textarea.value = block.content;
      if (diff && document.activeElement === textarea) {
        textarea.setSelectionRange(shiftOffset(start, diff), shiftOffset(end, diff));
      }
    }
    shown.current = block.content;
    textarea.style.height = "auto";
    textarea.style.height = `${textarea.scrollHeight}px`;
  }, [block.content, block.type]);

  useLayoutEffect(() => {
    if (focusRequested) {
      ref.current?.focus();
      onFocused();
    }
  }, [focusRequested, onFocused]);

  const items = menu ? filterSlashItems(menu.query) : [];

  function onChange(value: string) {
    const diff = diffText(shown.current, value);
    if (!diff || readOnly) return;
    if (diff.deleteCount > 0) actions.deleteText(block.block_id, diff.index, diff.deleteCount);
    if (diff.insert !== "") actions.insertText(block.block_id, diff.index, diff.insert);
    shown.current = value;
    // "/" typed at the start of an otherwise empty line opens the menu (frame 10).
    const match = /^\/(\w*)$/.exec(value);
    setMenu(match ? { query: match[1] ?? "", active: 0 } : null);
  }

  function pick(item: SlashItem) {
    // Remove the "/query" the user typed, then change the type.
    const length = block.content.length;
    if (length > 0) actions.deleteText(block.block_id, 0, length);
    shown.current = "";
    if (item.type !== block.type) actions.setType(block.block_id, item.type);
    setMenu(null);
  }

  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (readOnly) return;
    if (menu) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const count = Math.max(items.length, 1);
        const step = event.key === "ArrowDown" ? 1 : count - 1;
        setMenu({ ...menu, active: (menu.active + step) % count });
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        const item = items[menu.active];
        if (item) pick(item);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setMenu(null);
        return;
      }
    }
    const textarea = event.currentTarget;
    if (event.key === "Enter" && !event.shiftKey && block.type !== "CODE") {
      event.preventDefault();
      // Split at the caret: what follows the selection moves to a new block below.
      const from = textarea.selectionStart;
      const tail = block.content.slice(textarea.selectionEnd);
      const removeLength = block.content.length - from;
      if (removeLength > 0) actions.deleteText(block.block_id, from, removeLength);
      const id = actions.createBlockAfter(
        block.block_id,
        block.type === "CHECKLIST" ? "CHECKLIST" : "PARAGRAPH",
        tail,
      );
      if (id) onFocusBlock(id);
      return;
    }
    if (event.key === "Backspace" && block.content === "" && previousId) {
      event.preventDefault();
      actions.deleteBlock(block.block_id);
      onFocusBlock(previousId);
    }
  }

  const checked = block.properties.checked === true;
  const language = typeof block.properties.language === "string" ? block.properties.language : null;

  return (
    <div className="group relative flex items-start gap-2" data-block-id={block.block_id}>
      {!readOnly && (
        <button
          type="button"
          aria-label="Insert block below"
          onClick={() => {
            const id = actions.createBlockAfter(block.block_id, "PARAGRAPH", "");
            if (id) onFocusBlock(id);
          }}
          className="mt-1 w-5 shrink-0 text-text-faint opacity-0 group-hover:opacity-100 focus:opacity-100"
        >
          +
        </button>
      )}
      <div className="relative min-w-0 flex-1">
        {block.type === "CHECKLIST" && (
          <input
            type="checkbox"
            aria-label="Done"
            checked={checked}
            disabled={readOnly}
            onChange={() => actions.setProps(block.block_id, { checked: !checked })}
            className="absolute top-2 left-0"
          />
        )}
        {block.type === "CODE" && (
          <span className="mb-1 block text-micro text-text-muted uppercase">
            {language ?? "text"}
          </span>
        )}
        <textarea
          ref={ref}
          rows={1}
          defaultValue={block.content}
          readOnly={readOnly}
          aria-label={`${block.type.toLowerCase().replace("_", " ")} block`}
          placeholder={readOnly ? undefined : "Type / for commands"}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
          onBlur={() => setMenu(null)}
          spellCheck={block.type !== "CODE"}
          className={cn(
            "block w-full resize-none overflow-hidden bg-transparent text-text outline-none placeholder:text-text-faint",
            textClass[block.type],
            block.type === "CHECKLIST" && "pl-6",
            block.type === "CHECKLIST" && checked && "text-text-muted line-through",
          )}
        />
        {menu && !readOnly && <SlashMenu items={items} active={menu.active} onPick={pick} />}
      </div>
    </div>
  );
}
