"use client";

import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import type { ConnectionState } from "@/components/shell/ConnectionPill";

interface ConnectionValue {
  state: ConnectionState;
  report(state: ConnectionState): void;
}

const ConnectionContext = createContext<ConnectionValue | null>(null);

/** Holds the state of the open document socket so the sidebar pill can show it. */
export function ConnectionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ConnectionState>("connecting");
  const value = useMemo(() => ({ state, report: setState }), [state]);
  return <ConnectionContext.Provider value={value}>{children}</ConnectionContext.Provider>;
}

/** The state to show in the pill; "connecting" outside a provider. */
export function useConnectionState(): ConnectionState {
  return useContext(ConnectionContext)?.state ?? "connecting";
}

/**
 * Publishes the state of a socket while the calling component is mounted, and goes back to
 * "connecting" when it unmounts. Does nothing outside a provider.
 */
export function useReportConnection(state: ConnectionState): void {
  const report = useContext(ConnectionContext)?.report;
  useEffect(() => {
    report?.(state);
  }, [report, state]);
  useEffect(() => () => report?.("connecting"), [report]);
}
