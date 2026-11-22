import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { GithubButton, githubStartUrl } from "./GithubButton";

afterEach(() => vi.restoreAllMocks());

describe("githubStartUrl", () => {
  it("is the start route, with the page to return to when there is one", () => {
    expect(githubStartUrl()).toBe("/api/v1/auth/github/start");
    expect(githubStartUrl("/w/eng/repos")).toBe(
      "/api/v1/auth/github/start?next=%2Fw%2Feng%2Frepos",
    );
    expect(githubStartUrl("/w?x=1&y=2")).toBe(
      "/api/v1/auth/github/start?next=%2Fw%3Fx%3D1%26y%3D2",
    );
  });
});

describe("GithubButton", () => {
  it("shows a link to start the sign-in only when GitHub is set up", async () => {
    vi.spyOn(api.auth, "providers").mockResolvedValue({ github: true });
    render(<GithubButton next="/w/eng/repos" />);
    const link = await screen.findByRole("link", { name: "Continue with GitHub" });
    expect(link).toHaveAttribute("href", "/api/v1/auth/github/start?next=%2Fw%2Feng%2Frepos");
  });

  it("shows nothing while it asks, when GitHub is off, and when the answer fails", async () => {
    let resolve!: (value: { github: boolean }) => void;
    const providers = vi
      .spyOn(api.auth, "providers")
      .mockReturnValue(new Promise((r) => (resolve = r)));
    const view = render(<GithubButton />);
    expect(view.container).toBeEmptyDOMElement();
    await act(async () => resolve({ github: false }));
    expect(view.container).toBeEmptyDOMElement();
    view.unmount();

    providers.mockRejectedValue(new ApiError(502, "UNKNOWN", "bad gateway"));
    const failed = render(<GithubButton />);
    await act(async () => {});
    expect(failed.container).toBeEmptyDOMElement();
  });

  it("does nothing after it has been removed", async () => {
    let resolve!: (value: { github: boolean }) => void;
    vi.spyOn(api.auth, "providers").mockReturnValue(new Promise((r) => (resolve = r)));
    const view = render(<GithubButton />);
    view.unmount();
    await act(async () => resolve({ github: true }));
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });
});
