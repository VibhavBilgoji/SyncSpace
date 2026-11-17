import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import type { AuditEntry } from "@/types/generated";
import { ACTIONS, AuditPage, summary, utc } from "./AuditPage";

const api = vi.hoisted(() => ({ audit: { list: vi.fn() } }));

vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => ({ id: "ws1", slug: "eng", name: "Engineering", role: "ADMIN" }),
}));
vi.mock("@/lib/api", async () => {
  const errors = await import("@/lib/api/errors");
  return { api, isApiError: errors.isApiError };
});

function entry(n: number, overrides: Partial<AuditEntry> = {}): AuditEntry {
  return {
    event_id: `evt${String(n).padStart(21, "0")}`,
    actor: { id: "u1", name: "Ada Lovelace" },
    action: "DOC_CREATE",
    entity_type: "DOCUMENT",
    entity_id: "64b7f0a1c2d3e4f5a6b7c8d9",
    metadata: { title: "Plan" },
    timestamp: "2026-11-17T10:15:30.123Z",
    ...overrides,
  };
}

const page = (items: AuditEntry[], next: string | null = null) => ({ items, next_cursor: next });

beforeEach(() => {
  api.audit.list.mockReset();
  api.audit.list.mockResolvedValue(
    page([
      entry(2),
      entry(1, {
        action: "TASK_MOVE",
        entity_type: "TASK",
        metadata: { from_column: "TODO", to_column: "DONE", calculated_order: 2.5 },
      }),
    ]),
  );
});

describe("the audit trail (frame 39, reduced)", () => {
  it("shows the title, the append-only badge, and a row per event with the documented columns", async () => {
    render(<AuditPage />);
    expect(screen.getByLabelText("Loading audit trail")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "System Audit Trail" })).toBeInTheDocument();
    expect(screen.getByText("IMMUTABLE APPEND-ONLY")).toBeInTheDocument();
    for (const column of [
      "Timestamp (UTC)",
      "Actor",
      "Action",
      "Entity type",
      "Entity id",
      "State delta",
    ]) {
      expect(screen.getByRole("columnheader", { name: column })).toBeInTheDocument();
    }
    const rows = screen.getAllByRole("row");
    expect(rows).toHaveLength(3);
    const first = within(rows[1]!);
    expect(first.getByText("2026-11-17 10:15:30")).toBeInTheDocument();
    expect(first.getByText("Ada Lovelace")).toBeInTheDocument();
    expect(first.getByText("DOC_CREATE")).toBeInTheDocument();
    expect(first.getByText("DOCUMENT")).toBeInTheDocument();
    expect(first.getByText("64b7f0a1")).toHaveAttribute("title", "64b7f0a1c2d3e4f5a6b7c8d9");
    expect(first.getByText("title: Plan")).toBeInTheDocument();
    expect(
      within(rows[2]!).getByText("from_column: TODO, to_column: DONE, calculated_order: 2.5"),
    ).toBeInTheDocument();
    expect(api.audit.list).toHaveBeenCalledWith("ws1", { limit: 50 });
  });

  it("counts the events loaded, in the singular too", async () => {
    render(<AuditPage />);
    expect(await screen.findByText("2 events loaded")).toBeInTheDocument();
    api.audit.list.mockResolvedValue(page([entry(1)]));
    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "DOC_CREATE" } });
    expect(await screen.findByText("1 event loaded")).toBeInTheDocument();
  });

  it("filters by action, and All actions removes the filter", async () => {
    render(<AuditPage />);
    await screen.findByRole("heading", { name: "System Audit Trail" });
    const options = within(screen.getByLabelText("Action")).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["All actions", ...ACTIONS]);

    api.audit.list.mockResolvedValue(page([entry(3, { action: "TASK_MOVE" })]));
    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "TASK_MOVE" } });
    await waitFor(() =>
      expect(api.audit.list).toHaveBeenLastCalledWith("ws1", { limit: 50, action: "TASK_MOVE" }),
    );
    expect(await screen.findAllByText("TASK_MOVE")).toHaveLength(2); // the option and the pill
    expect(screen.getAllByRole("row")).toHaveLength(2);

    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "" } });
    await waitFor(() => expect(api.audit.list).toHaveBeenLastCalledWith("ws1", { limit: 50 }));
  });

  it("loads older events with the cursor and the same filter, and hides the button on the last page", async () => {
    api.audit.list.mockResolvedValueOnce(page([entry(3), entry(2)], "cursor-1"));
    api.audit.list.mockResolvedValueOnce(page([entry(1)], null));
    render(<AuditPage />);
    fireEvent.click(await screen.findByRole("button", { name: "Load older events" }));
    await waitFor(() => expect(screen.getAllByRole("row")).toHaveLength(4));
    expect(api.audit.list).toHaveBeenLastCalledWith("ws1", { limit: 50, cursor: "cursor-1" });
    expect(screen.getByText("3 events loaded")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load older events" })).not.toBeInTheDocument();
  });

  it("keeps the filter when paging", async () => {
    api.audit.list.mockResolvedValue(page([entry(2)], "c1"));
    render(<AuditPage />);
    await screen.findByRole("button", { name: "Load older events" });
    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "DOC_DELETE" } });
    fireEvent.click(await screen.findByRole("button", { name: "Load older events" }));
    await waitFor(() =>
      expect(api.audit.list).toHaveBeenLastCalledWith("ws1", {
        limit: 50,
        cursor: "c1",
        action: "DOC_DELETE",
      }),
    );
  });

  it("says nothing was recorded, with and without a filter", async () => {
    api.audit.list.mockResolvedValue(page([]));
    render(<AuditPage />);
    expect(await screen.findByText("No events")).toBeInTheDocument();
    expect(
      screen.getByText("Nothing has been recorded in this workspace yet."),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "TASK_DELETE" } });
    expect(
      await screen.findByText("No event of this action has been recorded yet."),
    ).toBeInTheDocument();
  });

  it("tells a non-admin that only admins can read it, and offers no filter", async () => {
    api.audit.list.mockRejectedValue(new ApiError(403, "FORBIDDEN", "forbidden"));
    render(<AuditPage />);
    expect(await screen.findByText("Only admins can read the audit trail.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Action")).not.toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  it("offers a retry after any other failure", async () => {
    api.audit.list.mockRejectedValueOnce(new ApiError(500, "INTERNAL", "internal error"));
    render(<AuditPage />);
    expect(await screen.findByText("Could not load the audit trail.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("table")).toBeInTheDocument();
  });

  it("has no export, no inspector and no actor or entity filter (reduced scope)", async () => {
    render(<AuditPage />);
    await screen.findByRole("table");
    expect(screen.queryByRole("button", { name: /export/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/actor|entity/i)).not.toBeInTheDocument();
    expect(screen.getAllByRole("combobox")).toHaveLength(1);
  });
});

describe("formatting", () => {
  it("shows UTC time to the second, whatever the fraction", () => {
    expect(utc("2026-11-17T10:15:30Z")).toBe("2026-11-17 10:15:30");
    expect(utc("2026-11-17T10:15:30.987654Z")).toBe("2026-11-17 10:15:30");
    expect(utc("1970-01-01T00:00:02Z")).toBe("1970-01-01 00:00:02");
  });

  it("summarises metadata on one line, and says none when there is none", () => {
    expect(summary({ title: "Plan", status: "TODO" })).toBe("title: Plan, status: TODO");
    expect(summary({ order: 2.5, nested: { a: 1 } })).toBe('order: 2.5, nested: {"a":1}');
    expect(summary({})).toBe("none");
  });
});
