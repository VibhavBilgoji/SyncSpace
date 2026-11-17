import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/Toast";
import { ApiError } from "@/lib/api/errors";
import type { Member } from "@/types/generated";
import { MembersPage } from "./MembersPage";

const ctx = vi.hoisted(() => ({ role: "ADMIN" as "ADMIN" | "EDITOR" | "VIEWER", me: "u1" }));
const api = vi.hoisted(() => ({
  workspaces: {
    members: vi.fn(),
    addMember: vi.fn(),
    changeRole: vi.fn(),
    removeMember: vi.fn(),
  },
}));

vi.mock("@/components/shell/WorkspaceShell", () => ({
  useWorkspace: () => ({ id: "ws1", slug: "eng", name: "Engineering", role: ctx.role }),
}));
vi.mock("@/lib/session", () => ({
  useSession: () => ({
    session: { status: "authed", user: { id: ctx.me, name: "Someone", email: "me@x.dev" } },
  }),
}));
vi.mock("@/lib/api", async () => {
  const errors = await import("@/lib/api/errors");
  return { api, isApiError: errors.isApiError };
});

function member(id: string, name: string, role: Member["role"], owner = false): Member {
  return {
    user_id: id,
    name,
    email: `${name.split(" ")[0]?.toLowerCase()}@x.dev`,
    role,
    joined_at: "2026-11-01T10:00:00Z",
    is_owner: owner,
  };
}

const members = [
  member("u1", "Ada Lovelace", "ADMIN", true),
  member("u2", "Grace Hopper", "EDITOR"),
  member("u3", "Alan Turing", "VIEWER"),
];

function renderPage() {
  return render(
    <ToastProvider>
      <MembersPage />
    </ToastProvider>,
  );
}

const row = (name: string) => screen.getByRole("row", { name: new RegExp(name) });

beforeEach(() => {
  ctx.role = "ADMIN";
  ctx.me = "u1";
  for (const fn of Object.values(api.workspaces)) fn.mockReset();
  api.workspaces.members.mockResolvedValue(members);
});

describe("the members table (frame 34)", () => {
  it("lists every member with email, role and joined date, and the note about owners", async () => {
    renderPage();
    expect(screen.getByLabelText("Loading members")).toBeInTheDocument();
    expect(await screen.findByText("Grace Hopper")).toBeInTheDocument();
    expect(screen.getByText("grace@x.dev")).toBeInTheDocument();
    expect(
      screen.getByText("Only admins can manage members. Owner access is locked."),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("row")).toHaveLength(4); // header and three members
    expect(api.workspaces.members).toHaveBeenCalledWith("ws1");
  });

  it("shows the owner as ADMIN (Owner) with a lock, and no role menu or Remove for the owner", async () => {
    renderPage();
    await screen.findByText("Grace Hopper");
    const owner = row("Ada Lovelace");
    expect(within(owner).getByText("ADMIN (Owner)")).toBeInTheDocument();
    expect(within(owner).queryByRole("combobox")).not.toBeInTheDocument();
    expect(within(owner).queryByRole("button")).not.toBeInTheDocument();
    expect(within(owner).getByText("(you)")).toBeInTheDocument();
  });

  it("gives an admin a role menu and a Remove button for every other member", async () => {
    renderPage();
    await screen.findByText("Grace Hopper");
    expect(screen.getByRole("button", { name: "+ Invite Member" })).toBeInTheDocument();
    expect(within(row("Grace Hopper")).getByLabelText("Role of Grace Hopper")).toHaveValue(
      "EDITOR",
    );
    expect(within(row("Alan Turing")).getByLabelText("Role of Alan Turing")).toHaveValue("VIEWER");
    expect(screen.getByRole("button", { name: "Remove Grace Hopper" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove Alan Turing" })).toBeInTheDocument();
  });

  it("never offers to remove the owner, or to change their role, to another admin", async () => {
    ctx.me = "u2";
    api.workspaces.members.mockResolvedValue([
      member("u1", "Ada Lovelace", "ADMIN", true),
      member("u2", "Grace Hopper", "ADMIN"),
      member("u3", "Alan Turing", "VIEWER"),
    ]);
    renderPage();
    await screen.findByText("Grace Hopper");
    const owner = row("Ada Lovelace");
    expect(within(owner).getByText("ADMIN (Owner)")).toBeInTheDocument();
    expect(within(owner).queryByRole("button")).not.toBeInTheDocument();
    expect(within(owner).queryByRole("combobox")).not.toBeInTheDocument();
    // Another admin may manage the others, but not remove themself from this screen.
    expect(within(row("Grace Hopper")).getByText("(you)")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove Grace Hopper" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove Alan Turing" })).toBeInTheDocument();
  });

  it.each(["EDITOR", "VIEWER"] as const)("shows %s the table read-only", async (role) => {
    ctx.role = role;
    renderPage();
    await screen.findByText("Grace Hopper");
    expect(screen.queryByRole("button", { name: "+ Invite Member" })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Remove/ })).not.toBeInTheDocument();
    expect(within(row("Grace Hopper")).getByText("EDITOR")).toBeInTheDocument();
    expect(within(row("Alan Turing")).getByText("VIEWER")).toBeInTheDocument();
  });

  it("offers a retry when the members cannot be loaded", async () => {
    api.workspaces.members.mockRejectedValueOnce(new Error("offline"));
    renderPage();
    expect(await screen.findByText("Could not load the members.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Grace Hopper")).toBeInTheDocument();
  });
});

describe("changing a role (frame 36)", () => {
  async function pick(name: string, role: string) {
    renderPage();
    await screen.findByText(name);
    fireEvent.change(within(row(name)).getByLabelText(`Role of ${name}`), {
      target: { value: role },
    });
    return screen.getByRole("dialog");
  }

  it("asks first, explains the roles and warns that open sessions change at once", async () => {
    const dialog = await pick("Grace Hopper", "ADMIN");
    expect(within(dialog).getByText("Change role of Grace Hopper?")).toBeInTheDocument();
    expect(within(dialog).getByText("EDITOR becomes ADMIN.")).toBeInTheDocument();
    expect(within(dialog).getByText(/Viewer: reads documents/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Admin: everything an editor can do/)).toBeInTheDocument();
    expect(
      within(dialog).getByText("Role changes apply to open sessions immediately."),
    ).toBeInTheDocument();
    expect(api.workspaces.changeRole).not.toHaveBeenCalled();
  });

  it("changes the role after confirmation and shows it in the row", async () => {
    api.workspaces.changeRole.mockResolvedValue(member("u2", "Grace Hopper", "ADMIN"));
    const dialog = await pick("Grace Hopper", "ADMIN");
    fireEvent.click(within(dialog).getByRole("button", { name: "Change role" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.workspaces.changeRole).toHaveBeenCalledWith("ws1", "u2", { role: "ADMIN" });
    expect(within(row("Grace Hopper")).getByLabelText("Role of Grace Hopper")).toHaveValue("ADMIN");
    expect(await screen.findByText("Grace Hopper is now ADMIN")).toBeInTheDocument();
  });

  it("changes nothing when cancelled, and the menu goes back to the real role", async () => {
    const dialog = await pick("Grace Hopper", "VIEWER");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.workspaces.changeRole).not.toHaveBeenCalled();
    expect(within(row("Grace Hopper")).getByLabelText("Role of Grace Hopper")).toHaveValue(
      "EDITOR",
    );
  });

  it("says so and reloads the members when the change fails", async () => {
    api.workspaces.changeRole.mockRejectedValue(new ApiError(500, "INTERNAL", "internal error"));
    const dialog = await pick("Grace Hopper", "VIEWER");
    fireEvent.click(within(dialog).getByRole("button", { name: "Change role" }));
    expect(await screen.findByText("Could not change the role")).toBeInTheDocument();
    await waitFor(() => expect(api.workspaces.members).toHaveBeenCalledTimes(2));
    expect(within(row("Grace Hopper")).getByLabelText("Role of Grace Hopper")).toHaveValue(
      "EDITOR",
    );
  });
});

describe("removing a member (frame 37)", () => {
  it("asks first and removes the row after confirmation", async () => {
    api.workspaces.removeMember.mockResolvedValue(undefined);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Remove Alan Turing" }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Remove Alan Turing?")).toBeInTheDocument();
    expect(api.workspaces.removeMember).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove member" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.workspaces.removeMember).toHaveBeenCalledWith("ws1", "u3");
    expect(screen.queryByText("Alan Turing")).not.toBeInTheDocument();
    expect(await screen.findByText("Alan Turing was removed")).toBeInTheDocument();
  });

  it("keeps the member when cancelled", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Remove Alan Turing" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(api.workspaces.removeMember).not.toHaveBeenCalled();
    expect(screen.getByText("Alan Turing")).toBeInTheDocument();
  });

  it("keeps the row and says so when the removal fails", async () => {
    api.workspaces.removeMember.mockRejectedValue(new ApiError(409, "CONFLICT", "owner"));
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Remove Alan Turing" }));
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Remove member" }),
    );
    expect(await screen.findByText("Could not remove the member")).toBeInTheDocument();
    expect(screen.getByText("Alan Turing")).toBeInTheDocument();
  });
});

describe("adding a member (frame 35)", () => {
  async function open() {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "+ Invite Member" }));
    return screen.getByRole("dialog");
  }

  it("needs an email, and offers Editor as the starting role", async () => {
    const dialog = await open();
    expect(within(dialog).getByRole("button", { name: "Add member" })).toBeDisabled();
    expect(within(dialog).getByLabelText("Role")).toHaveValue("EDITOR");
    fireEvent.change(within(dialog).getByLabelText("Email"), { target: { value: "new@x.dev" } });
    expect(within(dialog).getByRole("button", { name: "Add member" })).toBeEnabled();
  });

  it("adds the member with the trimmed email and chosen role and shows them in the table", async () => {
    api.workspaces.addMember.mockResolvedValue(member("u4", "Barbara Liskov", "VIEWER"));
    const dialog = await open();
    fireEvent.change(within(dialog).getByLabelText("Email"), {
      target: { value: "  barbara@x.dev " },
    });
    fireEvent.change(within(dialog).getByLabelText("Role"), { target: { value: "VIEWER" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Add member" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.workspaces.addMember).toHaveBeenCalledWith("ws1", {
      email: "barbara@x.dev",
      role: "VIEWER",
    });
    expect(screen.getByText("Barbara Liskov")).toBeInTheDocument();
    expect(await screen.findByText("Barbara Liskov was added as VIEWER")).toBeInTheDocument();
  });

  it.each([
    [404, "No SyncSpace account uses this email"],
    [409, "This person is already a member"],
  ])("says %i in words on the email field and keeps the dialog", async (status, message) => {
    api.workspaces.addMember.mockRejectedValue(new ApiError(status, "X", "x"));
    const dialog = await open();
    fireEvent.change(within(dialog).getByLabelText("Email"), { target: { value: "who@x.dev" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Add member" }));
    expect(await within(dialog).findByText(message)).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("explains a refused role and an unreachable server", async () => {
    api.workspaces.addMember.mockRejectedValueOnce(new ApiError(403, "FORBIDDEN", "forbidden"));
    const dialog = await open();
    fireEvent.change(within(dialog).getByLabelText("Email"), { target: { value: "who@x.dev" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Add member" }));
    expect(await within(dialog).findByText(/Only admins can add members/)).toBeInTheDocument();
    api.workspaces.addMember.mockRejectedValueOnce(new ApiError(0, "NETWORK", "offline"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add member" }));
    expect(await within(dialog).findByText(/Could not add the member/)).toBeInTheDocument();
  });

  it("cancels without adding anyone", async () => {
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.workspaces.addMember).not.toHaveBeenCalled();
  });
});
