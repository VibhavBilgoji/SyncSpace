import { act, render, renderHook, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConnectionPill } from "@/components/shell/ConnectionPill";
import { ApiError } from "@/lib/api/errors";
import { ConnectionProvider, useConnectionState } from "@/lib/connection";
import { connectionOf, useDocumentSession } from "./useDocumentSession";

const { wsTicket } = vi.hoisted(() => ({ wsTicket: vi.fn() }));

vi.mock("@/lib/api", () => ({ api: { auth: { wsTicket } } }));
vi.mock("@/lib/session", () => ({
  useSession: () => ({ session: { status: "authed", user: { name: "Ada Lovelace" } } }),
}));
vi.mock("@/wasm/pkg/sync_crdt", () => {
  class WasmDocument {
    syncStep1Frame = () => new Uint8Array([0, 0]);
    handleMessage = () => new Uint8Array();
    blocksJson = () => "[]";
    free = () => undefined;
  }
  return {
    default: () => Promise.resolve(),
    WasmDocument,
    awarenessFrame: () => new Uint8Array([1]),
    decodeAwareness: () => "[]",
    orderBetween: () => 1,
    updateFrame: (update: Uint8Array) => update,
  };
});

class FakeSocket {
  static readonly OPEN = 1;
  static instances: FakeSocket[] = [];
  readyState = 0;
  binaryType = "";
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  sent: unknown[] = [];
  constructor(readonly url: string) {
    FakeSocket.instances.push(this);
  }
  send(data: unknown) {
    this.sent.push(data);
  }
  close() {
    this.readyState = 3;
  }
  open() {
    this.readyState = FakeSocket.OPEN;
    this.onopen?.();
  }
  drop(code: number) {
    this.readyState = 3;
    this.onclose?.({ code } as CloseEvent);
  }
}

const wrapper = ({ children }: { children: ReactNode }) => (
  <ConnectionProvider>{children}</ConnectionProvider>
);

/** Lets the Wasm load and the ticket call settle, and moves the clock on by `ms`. */
async function settle(ms = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

function start() {
  const view = renderHook(() => useDocumentSession("doc1"), { wrapper });
  return view;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0.5); // no jitter: waits are exactly 0.5, 1, 2, 4, 8 s
  vi.stubGlobal("WebSocket", FakeSocket);
  FakeSocket.instances = [];
  wsTicket.mockReset();
  let n = 0;
  wsTicket.mockImplementation(() => Promise.resolve({ ticket: `t${(n += 1)}` }));
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("the document socket", () => {
  it("connects with a ticket, goes live on open and says so to the pill", async () => {
    const { result } = start();
    await settle();
    expect(result.current.status).toBe("connecting");
    expect(FakeSocket.instances).toHaveLength(1);
    expect(FakeSocket.instances[0]?.url).toContain("/ws/v1/docs/doc1?ticket=t1");
    act(() => FakeSocket.instances[0]?.open());
    expect(result.current.status).toBe("live");
  });

  it("reconnects after a drop with a new ticket and the documented backoff", async () => {
    const { result } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());

    // Every attempt that never opens waits twice as long, up to 8 s.
    const waits = [500, 1000, 2000, 4000, 8000, 8000];
    for (const [i, wait] of waits.entries()) {
      act(() => FakeSocket.instances[i]?.drop(1006));
      expect(result.current.status).toBe("reconnecting");
      await settle(wait - 1);
      expect(FakeSocket.instances).toHaveLength(i + 1); // not yet
      await settle(1);
      expect(FakeSocket.instances).toHaveLength(i + 2);
      expect(FakeSocket.instances[i + 1]?.url).toContain(`ticket=t${i + 2}`); // a new ticket each time
    }
    expect(wsTicket).toHaveBeenCalledTimes(waits.length + 1);
  });

  it("starts the backoff over once a connection has been established", async () => {
    const { result } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    act(() => FakeSocket.instances[0]?.drop(1006));
    await settle(500);
    act(() => FakeSocket.instances[1]?.drop(1006)); // never opened: the next wait is 1 s
    await settle(1000);
    act(() => FakeSocket.instances[2]?.open());
    expect(result.current.status).toBe("live");
    act(() => FakeSocket.instances[2]?.drop(1006));
    await settle(499);
    expect(FakeSocket.instances).toHaveLength(3);
    await settle(1);
    expect(FakeSocket.instances).toHaveLength(4); // 0.5 s again, not 2 s
  });

  it.each([
    [4004, "gone"],
    [4003, "forbidden"],
    [4001, "unauthorized"],
  ] as const)("does not reconnect after close code %i", async (code, status) => {
    const { result } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    act(() => FakeSocket.instances[0]?.drop(code));
    await settle(60_000);
    expect(result.current.status).toBe(status);
    expect(FakeSocket.instances).toHaveLength(1);
    expect(wsTicket).toHaveBeenCalledTimes(1);
  });

  it.each([1000, 1001, 1006, 1008, 1009])("reconnects after close code %i", async (code) => {
    const { result } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    act(() => FakeSocket.instances[0]?.drop(code));
    expect(result.current.status).toBe("reconnecting");
    await settle(500);
    expect(FakeSocket.instances).toHaveLength(2);
  });

  it("ends the session when the ticket is refused with 401, and retries when it fails otherwise", async () => {
    wsTicket.mockRejectedValueOnce(new ApiError(401, "UNAUTHORIZED", "not signed in"));
    const expired = start();
    await settle();
    expect(expired.result.current.status).toBe("unauthorized");
    await settle(60_000);
    expect(FakeSocket.instances).toHaveLength(0);
    expired.unmount();

    wsTicket.mockRejectedValueOnce(new TypeError("network down"));
    const offline = start();
    await settle();
    expect(offline.result.current.status).toBe("reconnecting");
    expect(FakeSocket.instances).toHaveLength(0);
    await settle(500);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it("stops retrying when the page is left", async () => {
    const { result, unmount } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    act(() => FakeSocket.instances[0]?.drop(1006));
    expect(result.current.status).toBe("reconnecting");
    unmount();
    await settle(60_000);
    expect(FakeSocket.instances).toHaveLength(1);
  });
});

describe("the connection pill", () => {
  function Probe() {
    const session = useDocumentSession("doc1");
    return <span data-testid="status">{session.status}</span>;
  }
  function Sidebar() {
    return <ConnectionPill state={useConnectionState()} />;
  }

  it("follows the socket: connecting, connected, reconnecting, disconnected, and back to connecting when the page unmounts", async () => {
    const view = render(
      <ConnectionProvider>
        <Sidebar />
        <Probe />
      </ConnectionProvider>,
    );
    await settle();
    expect(screen.getByRole("status")).toHaveTextContent("Connecting...");
    act(() => FakeSocket.instances[0]?.open());
    expect(screen.getByRole("status")).toHaveTextContent("Connected (WebSocket)");
    act(() => FakeSocket.instances[0]?.drop(1006));
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting...");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "reconnecting");
    await settle(500);
    act(() => FakeSocket.instances[1]?.drop(4003));
    expect(screen.getByRole("status")).toHaveTextContent("Disconnected");

    view.rerender(
      <ConnectionProvider>
        <Sidebar />
      </ConnectionProvider>,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Connecting...");
  });

  it("maps every session status to a pill state", () => {
    expect(connectionOf("live")).toBe("connected");
    expect(connectionOf("reconnecting")).toBe("reconnecting");
    expect(connectionOf("loading")).toBe("connecting");
    expect(connectionOf("connecting")).toBe("connecting");
    for (const ended of ["gone", "forbidden", "unauthorized", "failed"] as const) {
      expect(connectionOf(ended)).toBe("disconnected");
    }
  });
});
