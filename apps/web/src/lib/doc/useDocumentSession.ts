"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { ApiError } from "@/lib/api/errors";
import { useReportConnection } from "@/lib/connection";
import type { ConnectionState } from "@/components/shell/ConnectionPill";
import { retryDelay } from "./backoff";
import { useSession } from "@/lib/session";
import init, {
  WasmDocument,
  awarenessFrame,
  decodeAwareness,
  orderBetween,
  updateFrame,
} from "@/wasm/pkg/sync_crdt";
import type { Block, BlockType, ServerFrame } from "@/types/generated";

export type SessionStatus =
  | "loading"
  | "connecting"
  | "live"
  | "reconnecting"
  | "gone" // 4004: the document was deleted
  | "forbidden" // 4003: no longer a member
  | "unauthorized" // 4001
  | "failed";

export interface DocumentSession {
  status: SessionStatus;
  blocks: Block[];
  activeSockets: number;
  /** Names of the other people with this document open (awareness; no cursors). */
  peers: string[];
  /** Last snapshot the server reported: `rev_N · Saved to MongoDB`. */
  saved: { revision: number; savedAt: string } | null;
  /** The code of the last `ERROR` frame, for example READ_ONLY. */
  notice: string | null;
  insertText(blockId: string, index: number, chunk: string): void;
  deleteText(blockId: string, index: number, length: number): void;
  /** Inserts a block after `afterId` (or first when null) and returns its id. */
  createBlockAfter(afterId: string | null, type: BlockType, content: string): string | null;
  deleteBlock(blockId: string): void;
  setType(blockId: string, type: BlockType): void;
  setProps(blockId: string, props: Record<string, unknown>): void;
}

/** A random 53-bit replica id. Wasm has no randomness of its own. */
export function randomClientId(): number {
  const words = new Uint32Array(2);
  crypto.getRandomValues(words);
  const id = ((words[0] ?? 0) & 0x1fffff) * 2 ** 32 + (words[1] ?? 0);
  return id <= 1 ? 2 : id; // 1 is the server's own replica
}

const ID_ALPHABET = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/** A 12-character base62 block id (`docs/DATA_MODEL.md` section 6). */
export function randomBlockId(): string {
  const bytes = new Uint8Array(12);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => ID_ALPHABET[b % ID_ALPHABET.length]).join("");
}

const WS_BASE = process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:8080";
/** Presence is announced this often, and a peer not heard from for three times as long is gone. */
const PRESENCE_MS = 10_000;
const AWARENESS_TAG = 1; // the y-sync message type of an awareness frame

/** What the sidebar pill shows for a session status. */
export function connectionOf(status: SessionStatus): ConnectionState {
  switch (status) {
    case "live":
      return "connected";
    case "reconnecting":
      return "reconnecting";
    case "loading":
    case "connecting":
      return "connecting";
    default:
      return "disconnected";
  }
}

function readBlocks(doc: WasmDocument): Block[] {
  return JSON.parse(doc.blocksJson()) as Block[];
}

/**
 * Opens one document: loads the Wasm replica, asks for a ticket, connects, runs the y-sync
 * handshake and keeps `blocks` in step with the server. Every local edit is applied to the
 * replica first and then sent as an Update frame. Viewers get the same hook; the server refuses
 * their writes, and the page hides the controls. A dropped connection is retried with a new
 * ticket; edits made meanwhile stay in the replica and merge at the next handshake.
 */
export function useDocumentSession(docId: string): DocumentSession {
  const [status, setStatus] = useState<SessionStatus>("loading");
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [activeSockets, setActiveSockets] = useState(0);
  const [saved, setSaved] = useState<DocumentSession["saved"]>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [peers, setPeers] = useState<string[]>([]);
  const { session } = useSession();
  const myName = session.status === "authed" ? session.user.name : "Someone";
  const nameRef = useRef(myName);
  useEffect(() => {
    nameRef.current = myName;
  }, [myName]);
  const docRef = useRef<WasmDocument | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  useReportConnection(connectionOf(status));

  useEffect(() => {
    let cancelled = false;
    let attempt = 0;
    let retry: ReturnType<typeof setTimeout> | undefined;
    let doc: WasmDocument | null = null;
    let clientId = 0;
    let clock = 0;
    let heartbeat: ReturnType<typeof setInterval> | undefined;
    const seen = new Map<number, { name: string; at: number }>();

    const publishPeers = () => {
      const cutoff = Date.now() - PRESENCE_MS * 3;
      for (const [id, peer] of seen) if (peer.at < cutoff) seen.delete(id);
      setPeers([...seen.values()].map((p) => p.name).sort());
    };
    const announce = (ws: WebSocket, state: string) => {
      if (ws.readyState !== WebSocket.OPEN) return;
      try {
        clock += 1;
        ws.send(awarenessFrame(clientId, clock, state));
      } catch {
        // Presence is best effort: a refused frame must never break editing.
      }
    };

    const endOf = (code: number): SessionStatus | null =>
      code === 4004 ? "gone" : code === 4003 ? "forbidden" : code === 4001 ? "unauthorized" : null;

    function scheduleRetry() {
      if (cancelled) return;
      setStatus("reconnecting");
      const wait = retryDelay(attempt);
      attempt += 1;
      retry = setTimeout(() => void connect(), wait);
    }

    async function connect() {
      if (cancelled || !doc) return;
      setStatus(attempt === 0 ? "connecting" : "reconnecting");
      let ws: WebSocket;
      try {
        const { ticket } = await api.auth.wsTicket({ scope: "doc", id: docId });
        if (cancelled) return;
        ws = new WebSocket(`${WS_BASE}/ws/v1/docs/${docId}?ticket=${encodeURIComponent(ticket)}`);
      } catch (error) {
        if (cancelled) return;
        // An expired session cannot get a ticket: retrying would only repeat the 401.
        if (error instanceof ApiError && error.status === 401) setStatus("unauthorized");
        else scheduleRetry();
        return;
      }
      ws.binaryType = "arraybuffer";
      wsRef.current = ws;
      ws.onopen = () => {
        attempt = 0;
        setStatus("live");
        if (doc) ws.send(doc.syncStep1Frame());
        const hello = () => announce(ws, JSON.stringify({ name: nameRef.current }));
        hello();
        clearInterval(heartbeat);
        heartbeat = setInterval(() => {
          hello();
          publishPeers();
        }, PRESENCE_MS);
      };
      ws.onmessage = (event: MessageEvent) => {
        if (!doc) return;
        if (typeof event.data === "string") {
          const frame = JSON.parse(event.data) as ServerFrame;
          if (frame.type === "PRESENCE_COUNT") setActiveSockets(frame.active_sockets);
          else if (frame.type === "SNAPSHOT_SAVED")
            setSaved({ revision: frame.revision, savedAt: frame.saved_at });
          else if (frame.type === "ERROR") setNotice(frame.code);
          return;
        }
        const bytes = new Uint8Array(event.data as ArrayBuffer);
        if (bytes[0] === AWARENESS_TAG) {
          try {
            const entries = JSON.parse(decodeAwareness(bytes)) as Array<{
              client_id: number;
              state: { name?: unknown } | null;
            }>;
            for (const entry of entries) {
              if (entry.client_id === clientId) continue;
              const name = entry.state?.name;
              if (typeof name === "string" && name !== "") {
                seen.set(entry.client_id, { name: name.slice(0, 80), at: Date.now() });
              } else {
                seen.delete(entry.client_id);
              }
            }
            publishPeers();
          } catch {
            // A bad presence frame is ignored; it carries no document data.
          }
          return;
        }
        try {
          const reply = doc.handleMessage(bytes);
          if (reply.length > 0) ws.send(reply);
          setBlocks(readBlocks(doc));
        } catch (error) {
          // A frame that cannot be applied changes nothing; do not claim to be in sync.
          console.error("could not apply a frame from the server", error);
          setStatus("failed");
        }
      };
      ws.onclose = (event: CloseEvent) => {
        clearInterval(heartbeat);
        seen.clear();
        setPeers([]);
        if (cancelled) return;
        const end = endOf(event.code);
        if (end) setStatus(end);
        else scheduleRetry();
      };
    }

    init()
      .then(() => {
        if (cancelled) return;
        clientId = randomClientId();
        doc = new WasmDocument(clientId);
        docRef.current = doc;
        void connect();
      })
      .catch(() => !cancelled && setStatus("failed"));

    return () => {
      cancelled = true;
      clearTimeout(retry);
      clearInterval(heartbeat);
      if (wsRef.current) announce(wsRef.current, "null");
      wsRef.current?.close();
      wsRef.current = null;
      docRef.current = null;
      doc?.free();
    };
  }, [docId]);

  /** Applies a local edit and sends it. A refused edit (bad range, unknown block) changes nothing. */
  const edit = useCallback((run: (doc: WasmDocument) => Uint8Array): boolean => {
    const doc = docRef.current;
    if (!doc) return false;
    try {
      const update = run(doc);
      const ws = wsRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) ws.send(updateFrame(update));
      setBlocks(readBlocks(doc));
      setNotice(null);
      return true;
    } catch (error) {
      console.error("edit refused", error);
      return false;
    }
  }, []);

  const insertText = useCallback(
    (id: string, index: number, chunk: string) => void edit((d) => d.insertText(id, index, chunk)),
    [edit],
  );
  const deleteText = useCallback(
    (id: string, index: number, length: number) =>
      void edit((d) => d.deleteText(id, index, length)),
    [edit],
  );
  const deleteBlock = useCallback((id: string) => void edit((d) => d.deleteBlock(id)), [edit]);
  const setType = useCallback(
    (id: string, type: BlockType) => void edit((d) => d.setType(id, type)),
    [edit],
  );
  const setProps = useCallback(
    (id: string, props: Record<string, unknown>) =>
      void edit((d) => d.setProps(id, JSON.stringify(props))),
    [edit],
  );

  const createBlockAfter = useCallback(
    (afterId: string | null, type: BlockType, content: string) => {
      const doc = docRef.current;
      if (!doc) return null;
      const id = randomBlockId();
      const place = (d: WasmDocument) => {
        const current = readBlocks(d);
        const index = afterId === null ? -1 : current.findIndex((b) => b.block_id === afterId);
        const order = orderBetween(current[index]?.order, current[index + 1]?.order);
        return d.createBlock(id, type, order, content);
      };
      // `orderBetween` throws when the gap between neighbours is gone: renumber once, then retry.
      if (edit(place)) return id;
      edit((d) => d.renumber());
      return edit(place) ? id : null;
    },
    [edit],
  );

  return {
    status,
    blocks,
    activeSockets,
    peers,
    saved,
    notice,
    insertText,
    deleteText,
    createBlockAfter,
    deleteBlock,
    setType,
    setProps,
  };
}
