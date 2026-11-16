import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ada, renderWithSession } from "@/test/render";
import { RedirectIfAuthed, RequireAuth, useSession } from "./session";

const nav = vi.hoisted(() => ({ replace: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => nav }));

beforeEach(() => nav.replace.mockClear());

describe("RequireAuth", () => {
  it("shows a skeleton, then the page, for a signed-in user", async () => {
    renderWithSession(
      <RequireAuth>
        <p>private</p>
      </RequireAuth>,
      ada,
    );
    expect(screen.getByLabelText("Loading")).toBeInTheDocument();
    expect(await screen.findByText("private")).toBeInTheDocument();
    expect(nav.replace).not.toHaveBeenCalled();
  });

  it("redirects an anonymous visitor to /login and never renders the page", async () => {
    renderWithSession(
      <RequireAuth>
        <p>private</p>
      </RequireAuth>,
      null,
    );
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/login"));
    expect(screen.queryByText("private")).not.toBeInTheDocument();
  });
});

describe("RedirectIfAuthed", () => {
  it("shows the form to an anonymous visitor", async () => {
    renderWithSession(
      <RedirectIfAuthed>
        <p>login form</p>
      </RedirectIfAuthed>,
      null,
    );
    expect(await screen.findByText("login form")).toBeInTheDocument();
    expect(nav.replace).not.toHaveBeenCalled();
  });

  it("sends a signed-in user to the picker", async () => {
    renderWithSession(
      <RedirectIfAuthed>
        <p>login form</p>
      </RedirectIfAuthed>,
      ada,
    );
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/w"));
    expect(screen.queryByText("login form")).not.toBeInTheDocument();
  });
});

describe("useSession", () => {
  it("throws a clear error without a provider", () => {
    function Probe() {
      useSession();
      return null;
    }
    vi.spyOn(console, "error").mockImplementation(() => {});
    expect(() => render(<Probe />)).toThrow(/SessionProvider/);
  });
});
