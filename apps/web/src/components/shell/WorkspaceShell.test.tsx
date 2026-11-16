import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { ada, renderWithSession, workspaceSummary } from "@/test/render";
import { ConnectionPill } from "./ConnectionPill";
import { WorkspaceShell } from "./WorkspaceShell";

const nav = vi.hoisted(() => ({
  router: { replace: vi.fn() },
  params: { slug: "eng" },
  pathname: "/w/eng/docs",
}));
vi.mock("next/navigation", () => ({
  useRouter: () => nav.router,
  useParams: () => nav.params,
  usePathname: () => nav.pathname,
}));

beforeEach(() => nav.router.replace.mockClear());

function open(role: "ADMIN" | "EDITOR" | "VIEWER" = "ADMIN") {
  vi.spyOn(api.workspaces, "list").mockResolvedValue([workspaceSummary({ role })]);
  renderWithSession(
    <WorkspaceShell title="Documents">
      <p>page content</p>
    </WorkspaceShell>,
    ada,
  );
}

describe("WorkspaceShell", () => {
  it("shows the page inside the sidebar and header", async () => {
    open();
    expect(await screen.findByText("page content")).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toHaveTextContent(
      "Engineering Team / Documents",
    );
    expect(screen.getByRole("link", { name: /All documents/ })).toHaveAttribute(
      "href",
      "/w/eng/docs",
    );
    expect(screen.getByRole("link", { name: /All documents/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByText("Ada Lovelace")).toBeInTheDocument();
  });

  it("shows the Audit Trail only to ADMINs", async () => {
    open("ADMIN");
    expect(await screen.findByRole("link", { name: /Audit Trail/ })).toHaveAttribute(
      "href",
      "/w/eng/audit",
    );
  });

  it.each(["EDITOR", "VIEWER"] as const)("hides the Audit Trail from a %s", async (role) => {
    open(role);
    await screen.findByText("page content");
    expect(screen.queryByRole("link", { name: /Audit Trail/ })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Settings/ })).toHaveAttribute(
      "href",
      "/w/eng/settings",
    );
    expect(screen.getAllByText(role).length).toBeGreaterThan(0);
  });

  it("signs out and goes to /login", async () => {
    open();
    const logout = vi.spyOn(api.auth, "logout").mockResolvedValue();
    await screen.findByText("page content");
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    await waitFor(() => expect(nav.router.replace).toHaveBeenCalledWith("/login"));
    expect(logout).toHaveBeenCalled();
  });

  it("says not found for a slug the user has no access to", async () => {
    nav.params.slug = "secret";
    vi.spyOn(api.workspaces, "list").mockResolvedValue([workspaceSummary()]);
    renderWithSession(
      <WorkspaceShell>
        <p>page content</p>
      </WorkspaceShell>,
      ada,
    );
    expect(await screen.findByRole("heading", { name: "Workspace not found" })).toBeInTheDocument();
    expect(screen.queryByText("page content")).not.toBeInTheDocument();
    nav.params.slug = "eng";
  });

  it("shows an error with a retry when the list fails", async () => {
    vi.spyOn(api.workspaces, "list")
      .mockRejectedValueOnce(new ApiError(500, "INTERNAL", "boom"))
      .mockResolvedValue([workspaceSummary()]);
    renderWithSession(
      <WorkspaceShell>
        <p>page content</p>
      </WorkspaceShell>,
      ada,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load the workspace");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("page content")).toBeInTheDocument();
  });

  it("sends a signed-out visitor to /login", async () => {
    renderWithSession(
      <WorkspaceShell>
        <p>page content</p>
      </WorkspaceShell>,
      null,
    );
    await waitFor(() => expect(nav.router.replace).toHaveBeenCalledWith("/login"));
    expect(screen.queryByText("page content")).not.toBeInTheDocument();
  });
});

describe("ConnectionPill", () => {
  it("defaults to connecting", () => {
    render_pill();
    expect(screen.getByRole("status")).toHaveTextContent("Connecting...");
  });

  it("shows connected with latency, and reconnecting", () => {
    const { rerender } = render_pill("connected", 5);
    expect(screen.getByRole("status")).toHaveTextContent("Connected (WebSocket) 5ms");
    rerender(<ConnectionPill state="reconnecting" />);
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting...");
    expect(screen.getByRole("status")).toHaveAttribute("data-state", "reconnecting");
  });
});

import { render } from "@testing-library/react";
function render_pill(state?: "connected" | "reconnecting" | "connecting", latencyMs?: number) {
  return render(<ConnectionPill state={state} latencyMs={latencyMs} />);
}
