import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { ada, renderWithSession } from "@/test/render";
import { OnboardingForm } from "./OnboardingForm";

const nav = vi.hoisted(() => ({ replace: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => nav }));

beforeEach(() => nav.replace.mockClear());

async function open() {
  renderWithSession(<OnboardingForm />, ada);
  await screen.findByLabelText("Workspace name");
}

const name = () => screen.getByLabelText("Workspace name") as HTMLInputElement;
const slug = () => screen.getByLabelText("Slug") as HTMLInputElement;
const submit = () => screen.getByRole("button", { name: "Create workspace" });

describe("OnboardingForm", () => {
  it("sends a signed-out visitor to /login", async () => {
    renderWithSession(<OnboardingForm />, null);
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/login"));
    expect(screen.queryByLabelText("Workspace name")).not.toBeInTheDocument();
  });

  it("fills the slug from the name until the slug is edited by hand", async () => {
    await open();
    expect(submit()).toBeDisabled();
    fireEvent.change(name(), { target: { value: "Engineering Team" } });
    expect(slug().value).toBe("engineering-team");
    expect(submit()).toBeEnabled();
    fireEvent.change(slug(), { target: { value: "eng" } });
    fireEvent.change(name(), { target: { value: "Engineering Team 2" } });
    expect(slug().value).toBe("eng");
  });

  it("flags a slug that breaks the rule and blocks submit", async () => {
    await open();
    fireEvent.change(name(), { target: { value: "Team" } });
    fireEvent.change(slug(), { target: { value: "Bad Slug" } });
    expect(screen.getByRole("alert")).toHaveTextContent("3 to 40 lowercase");
    expect(submit()).toBeDisabled();
  });

  it("creates the workspace and opens its documents", async () => {
    const create = vi.spyOn(api.workspaces, "create").mockResolvedValue({
      id: "b".repeat(24),
      name: "Engineering Team",
      slug: "engineering-team",
      owner_id: ada.id,
      role: "ADMIN",
      created_at: "2026-11-16T00:00:00Z",
      updated_at: "2026-11-16T00:00:00Z",
    });
    await open();
    fireEvent.change(name(), { target: { value: " Engineering Team " } });
    fireEvent.click(submit());
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/w/engineering-team/docs"));
    expect(create).toHaveBeenCalledWith({ name: "Engineering Team", slug: "engineering-team" });
  });

  it("shows a taken slug under the slug field", async () => {
    vi.spyOn(api.workspaces, "create").mockRejectedValue(new ApiError(409, "CONFLICT", "taken"));
    await open();
    fireEvent.change(name(), { target: { value: "Team" } });
    fireEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent("already taken");
    expect(slug()).toHaveAttribute("aria-invalid", "true");
    expect(nav.replace).not.toHaveBeenCalledWith(expect.stringContaining("/docs"));
  });
});
