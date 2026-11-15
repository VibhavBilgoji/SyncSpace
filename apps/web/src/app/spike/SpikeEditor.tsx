"use client";

// THROWAWAY (roadmap task 0.3): a textarea bound to one shared Y.Text through the Wasm
// build of sync-crdt and the spike relay. Delete with the relay when the real editor lands.

import { useEffect, useRef, useState } from "react";
import init, { WasmDoc } from "@/wasm/pkg/sync_crdt";
import { diffText, shiftOffset } from "@/lib/textDiff";

const WS_URL = `${process.env.NEXT_PUBLIC_WS_URL ?? "ws://localhost:8080"}/spike`;

type Status = "loading" | "connecting" | "connected" | "disconnected" | "error";

/** A random 53-bit id. Wasm has no randomness of its own, so the page supplies it. */
function randomClientId(): number {
  const words = new Uint32Array(2);
  crypto.getRandomValues(words);
  const id = ((words[0] ?? 0) & 0x1fffff) * 2 ** 32 + (words[1] ?? 0);
  return id === 0 ? 1 : id; // 0 is the relay's own replica
}

export default function SpikeEditor() {
  const [status, setStatus] = useState<Status>("loading");
  const [length, setLength] = useState(0);
  // True once the relay's full state has been applied, so tests do not race it.
  const [synced, setSynced] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const docRef = useRef<WasmDoc | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  // What the textarea showed after the last sync, so each edit can be diffed against it.
  const shownRef = useRef("");

  useEffect(() => {
    let cancelled = false;
    let doc: WasmDoc | null = null;
    let ws: WebSocket | null = null;

    // Show the document's text in the textarea, keeping the caret where the user left it.
    const refreshFromDoc = () => {
      const textarea = textareaRef.current;
      if (!textarea || !doc) return;
      const next = doc.text();
      const diff = diffText(shownRef.current, next);
      if (diff) {
        const start = shiftOffset(textarea.selectionStart, diff);
        const end = shiftOffset(textarea.selectionEnd, diff);
        textarea.value = next;
        textarea.setSelectionRange(start, end);
      }
      shownRef.current = next;
      setLength(next.length);
    };

    init()
      .then(() => {
        if (cancelled) return;
        doc = new WasmDoc(randomClientId());
        docRef.current = doc;
        setStatus("connecting");

        ws = new WebSocket(WS_URL);
        ws.binaryType = "arraybuffer";
        wsRef.current = ws;
        ws.onopen = () => {
          setStatus("connected");
          // Share anything typed while connecting; applying an update twice is harmless.
          if (doc && doc.text() !== "") ws?.send(doc.encodeStateAsUpdate());
        };
        ws.onclose = () => setStatus("disconnected");
        ws.onmessage = (event: MessageEvent) => {
          if (!(event.data instanceof ArrayBuffer) || !doc) return;
          try {
            doc.applyUpdate(new Uint8Array(event.data));
          } catch (error) {
            // An invalid or oversized update changes nothing. Say so instead of showing a
            // silently empty editor next to a "connected" status.
            console.error("could not apply an update from the relay", error);
            setStatus("error");
            return;
          }
          refreshFromDoc();
          // The first frame is the relay's full state; after it the editor is in sync.
          setSynced(true);
        };
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        console.error("could not load the Wasm module", error);
        setStatus("error");
      });

    return () => {
      cancelled = true;
      if (ws) {
        // A socket that is closing must not update the state of a newer mount.
        ws.onopen = null;
        ws.onclose = null;
        ws.onmessage = null;
        ws.close();
      }
      wsRef.current = null;
      docRef.current = null;
      const finished = doc;
      doc = null; // late callbacks see no document instead of a freed one
      finished?.free();
    };
  }, []);

  const onInput = () => {
    const textarea = textareaRef.current;
    const doc = docRef.current;
    if (!textarea || !doc) return;
    const diff = diffText(shownRef.current, textarea.value);
    if (!diff) return;
    const send = (update: Uint8Array) => {
      if (wsRef.current?.readyState === WebSocket.OPEN) wsRef.current.send(update);
    };
    try {
      if (diff.deleteCount > 0) send(doc.delete(diff.index, diff.deleteCount));
      if (diff.insert !== "") send(doc.insert(diff.index, diff.insert));
    } catch {
      // The textarea and the document disagree: trust the document.
      textarea.value = doc.text();
    }
    shownRef.current = doc.text();
    setLength(shownRef.current.length);
  };

  return (
    <div>
      <p data-testid="spike-status" data-synced={synced}>
        {status}
      </p>
      <textarea
        ref={textareaRef}
        data-testid="spike-text"
        aria-label="Shared text"
        disabled={status === "loading" || status === "error"}
        rows={8}
        cols={60}
        onInput={onInput}
      />
      <p data-testid="spike-length">{length} characters</p>
    </div>
  );
}
