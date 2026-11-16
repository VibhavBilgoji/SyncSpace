import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TOAST_DURATION_MS, ToastProvider, useToast, type ToastInput } from "./Toast";

function Trigger({ input }: { input: ToastInput }) {
  const { toast } = useToast();
  return <button onClick={() => toast(input)}>Fire</button>;
}

function setup(input: ToastInput) {
  render(
    <ToastProvider>
      <Trigger input={input} />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Fire" }));
}

afterEach(() => vi.useRealTimers());

describe("Toast", () => {
  it("shows a status toast with its description", () => {
    setup({ title: "Saved", description: "Version 4 created.", tone: "success" });
    expect(screen.getByRole("status")).toHaveTextContent("Saved");
    expect(screen.getByRole("status")).toHaveTextContent("Version 4 created.");
  });

  it("announces danger toasts as alerts", () => {
    setup({ title: "Your role was changed to VIEWER", tone: "danger" });
    expect(screen.getByRole("alert")).toHaveTextContent("VIEWER");
  });

  it("can be dismissed by hand", () => {
    setup({ title: "Saved" });
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("disappears by itself after the duration", () => {
    vi.useFakeTimers();
    setup({ title: "Saved" });
    expect(screen.getByRole("status")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(TOAST_DURATION_MS + 1);
    });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("throws a clear error without a provider", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Trigger input={{ title: "x" }} />)).toThrow(/ToastProvider/);
    spy.mockRestore();
  });
});
