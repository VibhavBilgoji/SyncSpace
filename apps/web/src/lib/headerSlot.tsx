"use client";

import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

interface SlotValue {
  element: HTMLElement | null;
  setElement(element: HTMLElement | null): void;
}

const SlotContext = createContext<SlotValue | null>(null);

/** Lets a page put controls into the app header (UI_SPEC section 3: avatars, Version History, Share). */
export function HeaderSlotProvider({ children }: { children: ReactNode }) {
  const [element, setElement] = useState<HTMLElement | null>(null);
  const value = useMemo(() => ({ element, setElement }), [element]);
  return <SlotContext.Provider value={value}>{children}</SlotContext.Provider>;
}

/** The place in the header where page controls appear; rendered once, by the header. */
export function HeaderSlot() {
  const slot = useContext(SlotContext);
  return <div ref={slot?.setElement} data-header-slot className="flex items-center gap-3" />;
}

/** Renders `children` into the header slot. Renders nothing when there is no header. */
export function HeaderActions({ children }: { children: ReactNode }) {
  const element = useContext(SlotContext)?.element;
  return element ? createPortal(children, element) : null;
}
