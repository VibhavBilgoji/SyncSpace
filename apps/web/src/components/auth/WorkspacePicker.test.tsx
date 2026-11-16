import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { ada, renderWithSession, workspaceSummary } from "@/test/render";
import { WorkspacePicker } from "./WorkspacePicker";

const nav = vi.hoisted(() => ({ replace: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => nav }));

beforeEach(() => nav.replace.mockClear());

describe("WorkspacePicker", () => {
  it("shows a card per workspace with role and counts", async () => {
    vi.spyOn(api.workspaces, "list").mockResolvedValue([
      workspaceSummary(),
      workspaceSummary({
        id: "c".repeat(24),
        name: "Design",
        slug: "design",
        role: "VIEWER",
        member_count: 8,
      }),
    ]);
    renderWithSession(<WorkspacePicker />, ada);
    const card = await screen.findByRole("link", { name: /Engineering Team/ });
    expect(card).toHaveAttribute("href", "/w/eng/docs");
    expect(card).toHaveTextContent("ADMIN");
    expect(card).toHaveTextContent("3 members · 5 documents · 1 boards");
    expect(screen.getByRole("link", { name: /Design/ })).toHaveTextContent("VIEWER");
  });

  it("shows a skeleton while loading", () => {
    vi.spyOn(api.workspaces, "list").mockReturnValue(new Promise(() => {}));
    renderWithSession(<WorkspacePicker />, ada);
    return screen.findByLabelText("Loading workspaces");
  });

  it("sends a user with no workspace to onboarding", async () => {
    vi.spyOn(api.workspaces, "list").mockResolvedValue([]);
    renderWithSession(<WorkspacePicker />, ada);
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/onboarding"));
  });

  it("shows an error with a working retry", async () => {
    const list = vi
      .spyOn(api.workspaces, "list")
      .mockRejectedValueOnce(new ApiError(0, "NETWORK", "offline"))
      .mockResolvedValue([workspaceSummary()]);
    renderWithSession(<WorkspacePicker />, ada);
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load your workspaces");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("link", { name: /Engineering Team/ })).toBeInTheDocument();
    expect(list).toHaveBeenCalledTimes(2);
  });

  it("sends a signed-out visitor to /login without loading anything", async () => {
    const list = vi.spyOn(api.workspaces, "list");
    renderWithSession(<WorkspacePicker />, null);
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/login"));
    expect(list).not.toHaveBeenCalled();
  });
});
