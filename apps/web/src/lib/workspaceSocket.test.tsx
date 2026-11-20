import type { ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import { ConnectionProvider, useConnectionState } from "@/lib/connection";
import { connectionOf, useWorkspaceSocket } from "./workspaceSocket";

const { wsTicket } = vi.hoisted(() => ({ wsTicket: vi.fn() }));
vi.mock("@/lib/api", () => ({ api: { auth: { wsTicket } } }));

class FakeSocket {
  static readonly OPEN = 1;
  static instances: FakeSocket[] = [];
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: ((event: CloseEvent) => void) | null = null;
  sent: string[] = [];
  closed = false;
  constructor(readonly url: string) {
    FakeSocket.instances.push(this);
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.closed = true;
    this.readyState = 3;
  }
  open() {
    this.readyState = FakeSocket.OPEN;
    this.onopen?.();
  }
  say(data: unknown) {
    this.onmessage?.({
      data: typeof data === "string" ? data : JSON.stringify(data),
    } as MessageEvent);
  }
  drop(code: number) {
    this.readyState = 3;
    this.onclose?.({ code } as CloseEvent);
  }
}

async function settle(ms = 0) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

function start(onFrame = vi.fn(), onConnected = vi.fn()) {
  const view = renderHook(() => useWorkspaceSocket("ws1", { onFrame, onConnected }));
  return { ...view, onFrame, onConnected };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0.5);
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

describe("useWorkspaceSocket", () => {
  it("asks for a workspace ticket, connects to the workspace URL and goes live", async () => {
    const { result, onConnected } = start();
    expect(result.current).toBe("connecting");
    await settle();
    expect(wsTicket).toHaveBeenCalledWith({ scope: "workspace", id: "ws1" });
    expect(FakeSocket.instances[0]?.url).toContain("/ws/v1/workspaces/ws1?ticket=t1");
    act(() => FakeSocket.instances[0]?.open());
    expect(result.current).toBe("live");
    expect(onConnected).toHaveBeenCalledWith(false);
  });

  it("hands every frame to the page, except PONG, junk and binary", async () => {
    const { onFrame } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    const event = { type: "BOARD_EVENT", board_id: "b1", kind: "TASK_MOVED" };
    act(() => {
      FakeSocket.instances[0]?.say(event);
      FakeSocket.instances[0]?.say({ type: "PONG", t: 1 });
      FakeSocket.instances[0]?.say("not json");
      FakeSocket.instances[0]?.onmessage?.({ data: new ArrayBuffer(4) } as MessageEvent);
    });
    expect(onFrame).toHaveBeenCalledTimes(1);
    expect(onFrame).toHaveBeenCalledWith(event);
  });

  it("pings every 20 seconds while open, and stops when closed", async () => {
    start();
    await settle();
    const socket = FakeSocket.instances[0]!;
    act(() => socket.open());
    await settle(19_999);
    expect(socket.sent).toHaveLength(0);
    await settle(1);
    expect(socket.sent).toHaveLength(1);
    expect(JSON.parse(socket.sent[0]!)).toMatchObject({ type: "PING" });
    await settle(20_000);
    expect(socket.sent).toHaveLength(2);
    act(() => socket.drop(1006));
    await settle(60_000);
    expect(socket.sent).toHaveLength(2);
  });

  it("reconnects after a drop with a new ticket and tells the page it is a reconnection", async () => {
    const { result, onConnected } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    act(() => FakeSocket.instances[0]?.drop(1006));
    expect(result.current).toBe("reconnecting");
    await settle(500);
    expect(FakeSocket.instances).toHaveLength(2);
    expect(FakeSocket.instances[1]?.url).toContain("ticket=t2");
    act(() => FakeSocket.instances[1]?.open());
    expect(result.current).toBe("live");
    expect(onConnected).toHaveBeenLastCalledWith(true);
  });

  it("backs off 0.5, 1, 2 s when connections keep failing, and starts over after one opens", async () => {
    start();
    await settle();
    act(() => FakeSocket.instances[0]?.drop(1006));
    await settle(499);
    expect(FakeSocket.instances).toHaveLength(1);
    await settle(1);
    expect(FakeSocket.instances).toHaveLength(2);
    act(() => FakeSocket.instances[1]?.drop(1006));
    await settle(999);
    expect(FakeSocket.instances).toHaveLength(2);
    await settle(1);
    expect(FakeSocket.instances).toHaveLength(3);
    act(() => FakeSocket.instances[2]?.open());
    act(() => FakeSocket.instances[2]?.drop(1006));
    await settle(500);
    expect(FakeSocket.instances).toHaveLength(4);
  });

  it.each([4001, 4003, 4004])("stops for good on close code %i", async (code) => {
    const { result } = start();
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    act(() => FakeSocket.instances[0]?.drop(code));
    await settle(60_000);
    expect(result.current).toBe("ended");
    expect(FakeSocket.instances).toHaveLength(1);
    expect(wsTicket).toHaveBeenCalledTimes(1);
  });

  it.each([401, 404])("ends when the ticket is refused with %i", async (status) => {
    wsTicket.mockRejectedValueOnce(new ApiError(status, "X", "x"));
    const { result } = start();
    await settle();
    expect(result.current).toBe("ended");
    await settle(60_000);
    expect(FakeSocket.instances).toHaveLength(0);
  });

  it("retries when the ticket call fails for another reason", async () => {
    wsTicket.mockRejectedValueOnce(new TypeError("offline"));
    const { result } = start();
    await settle();
    expect(result.current).toBe("reconnecting");
    await settle(500);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it("closes the socket and stops retrying when the page is left", async () => {
    const { unmount } = start();
    await settle();
    const socket = FakeSocket.instances[0]!;
    act(() => socket.open());
    unmount();
    expect(socket.closed).toBe(true);
    act(() => socket.drop(1006));
    await settle(60_000);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it("uses the page's newest handler without reconnecting", async () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender } = renderHook(({ onFrame }) => useWorkspaceSocket("ws1", { onFrame }), {
      initialProps: { onFrame: first },
    });
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    rerender({ onFrame: second });
    act(() => FakeSocket.instances[0]?.say({ type: "BOARD_EVENT" }));
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it("opens a new socket when the workspace changes", async () => {
    const onFrame = vi.fn();
    const { rerender } = renderHook(({ id }) => useWorkspaceSocket(id, { onFrame }), {
      initialProps: { id: "ws1" },
    });
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    rerender({ id: "ws2" });
    await settle();
    expect(FakeSocket.instances[0]?.closed).toBe(true);
    expect(FakeSocket.instances[1]?.url).toContain("/ws/v1/workspaces/ws2?");
  });

  it("maps its status to the sidebar pill", () => {
    expect(connectionOf("connecting")).toBe("connecting");
    expect(connectionOf("live")).toBe("connected");
    expect(connectionOf("reconnecting")).toBe("reconnecting");
    expect(connectionOf("ended")).toBe("disconnected");
  });

  it("shows the socket's state in the pill while the page is open, and resets it on leaving", async () => {
    const wrapper = ({ children }: { children: ReactNode }) => (
      <ConnectionProvider>{children}</ConnectionProvider>
    );
    const view = renderHook(
      () => {
        const status = useWorkspaceSocket("ws1", { onFrame: vi.fn() });
        return { status, pill: useConnectionState() };
      },
      { wrapper },
    );
    expect(view.result.current.pill).toBe("connecting");
    await settle();
    act(() => FakeSocket.instances[0]?.open());
    expect(view.result.current.pill).toBe("connected");
    act(() => FakeSocket.instances[0]?.drop(1006));
    expect(view.result.current.pill).toBe("reconnecting");
  });
});
