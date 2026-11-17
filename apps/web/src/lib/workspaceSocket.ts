"use client";

import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { ApiError } from "@/lib/api/errors";
import { retryDelay } from "@/lib/doc/backoff";
import type { ServerFrame } from "@/types/generated";

const WS_BASE = process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:8080";
/** `docs/API.md`: the client sends a `PING` every 20 seconds. */
const PING_MS = 20_000;

export type WorkspaceSocketStatus = "connecting" | "live" | "reconnecting" | "ended";

interface Handlers {
  /** Every frame the server sends except `PONG`. */
  onFrame(frame: ServerFrame): void;
  /**
   * The socket is open. `again` is true after a drop: events sent while it was down are lost, so
   * the page should load its state again.
   */
  onConnected?(again: boolean): void;
}

/**
 * The workspace socket (`docs/API.md` section 3): live events of one workspace. It asks for a
 * ticket, connects, pings, and after a drop reconnects with the backoff of the document socket
 * and a new ticket. It stops for good on 4001, 4003 and 4004 (the page learns the reason from
 * REST) and when the ticket is refused with 401.
 */
export function useWorkspaceSocket(workspaceId: string, handlers: Handlers): WorkspaceSocketStatus {
  const [status, setStatus] = useState<WorkspaceSocketStatus>("connecting");
  const handlersRef = useRef(handlers);
  useEffect(() => {
    handlersRef.current = handlers;
  });

  useEffect(() => {
    let cancelled = false;
    let attempt = 0;
    let everOpen = false;
    let retry: ReturnType<typeof setTimeout> | undefined;
    let pinger: ReturnType<typeof setInterval> | undefined;
    let socket: WebSocket | null = null;

    function scheduleRetry() {
      if (cancelled) return;
      setStatus("reconnecting");
      retry = setTimeout(() => void connect(), retryDelay(attempt));
      attempt += 1;
    }

    async function connect() {
      if (cancelled) return;
      let ws: WebSocket;
      try {
        const { ticket } = await api.auth.wsTicket({ scope: "workspace", id: workspaceId });
        if (cancelled) return;
        ws = new WebSocket(
          `${WS_BASE}/ws/v1/workspaces/${workspaceId}?ticket=${encodeURIComponent(ticket)}`,
        );
      } catch (error) {
        if (cancelled) return;
        if (error instanceof ApiError && (error.status === 401 || error.status === 404)) {
          setStatus("ended");
        } else {
          scheduleRetry();
        }
        return;
      }
      socket = ws;
      ws.onopen = () => {
        attempt = 0;
        setStatus("live");
        const again = everOpen;
        everOpen = true;
        handlersRef.current.onConnected?.(again);
        clearInterval(pinger);
        pinger = setInterval(() => {
          if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ type: "PING", t: Date.now() }));
          }
        }, PING_MS);
      };
      ws.onmessage = (event: MessageEvent) => {
        if (typeof event.data !== "string") return;
        let frame: ServerFrame;
        try {
          frame = JSON.parse(event.data) as ServerFrame;
        } catch {
          return;
        }
        if (frame.type === "PONG") return;
        handlersRef.current.onFrame(frame);
      };
      ws.onclose = (event: CloseEvent) => {
        clearInterval(pinger);
        if (cancelled) return;
        if (event.code === 4001 || event.code === 4003 || event.code === 4004) {
          setStatus("ended");
        } else {
          scheduleRetry();
        }
      };
    }

    void connect();
    return () => {
      cancelled = true;
      clearTimeout(retry);
      clearInterval(pinger);
      socket?.close();
    };
  }, [workspaceId]);

  return status;
}
