import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { ada, renderWithSession } from "@/test/render";
import { GITHUB_ERRORS, LoginForm, WAKE_ATTEMPTS, WAKE_WAIT_MS } from "./LoginForm";

const nav = vi.hoisted(() => ({ replace: vi.fn(), query: { value: "" } }));
vi.mock("next/navigation", () => ({
  useRouter: () => nav,
  useSearchParams: () => new URLSearchParams(nav.query.value),
}));

beforeEach(() => {
  nav.replace.mockClear();
  nav.query.value = "";
  vi.spyOn(api.auth, "providers").mockResolvedValue({ github: false });
});

function fill(email: string, password: string) {
  fireEvent.change(screen.getByLabelText("Email"), { target: { value: email } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: password } });
}

const submit = () => screen.getByRole("button", { name: "Sign in" });

describe("LoginForm", () => {
  it("keeps submit disabled until both fields are filled", () => {
    renderWithSession(<LoginForm />);
    expect(submit()).toBeDisabled();
    fill("ada@example.com", "");
    expect(submit()).toBeDisabled();
    fill("ada@example.com", "secret");
    expect(submit()).toBeEnabled();
  });

  it("signs in, trims the email and goes to the picker", async () => {
    const login = vi.spyOn(api.auth, "login").mockResolvedValue(ada);
    renderWithSession(<LoginForm />);
    fill("  ada@example.com ", "secret");
    fireEvent.click(submit());
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/w"));
    expect(login).toHaveBeenCalledWith({ email: "ada@example.com", password: "secret" });
  });

  it("shows the loading state while the request is pending", async () => {
    vi.spyOn(api.auth, "login").mockReturnValue(new Promise(() => {}));
    renderWithSession(<LoginForm />);
    fill("ada@example.com", "secret");
    fireEvent.click(submit());
    await waitFor(() => expect(submit()).toHaveAttribute("aria-busy", "true"));
    expect(submit()).toBeDisabled();
  });

  it("shows a wrong-credentials error inline and lets the user retry", async () => {
    vi.spyOn(api.auth, "login").mockRejectedValue(
      new ApiError(401, "UNAUTHORIZED", "invalid email or password"),
    );
    renderWithSession(<LoginForm />);
    fill("ada@example.com", "wrong");
    fireEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent("Invalid email or password.");
    expect(nav.replace).not.toHaveBeenCalled();
    expect(submit()).toBeEnabled();
  });

  it("explains a rate limit and an unexpected failure differently, and retries neither", async () => {
    const login = vi
      .spyOn(api.auth, "login")
      .mockRejectedValueOnce(new ApiError(429, "RATE_LIMITED", "slow down"))
      .mockRejectedValueOnce(new ApiError(400, "BAD_REQUEST", "odd"));
    renderWithSession(<LoginForm />);
    fill("ada@example.com", "secret");
    fireEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent("Too many attempts");
    fireEvent.click(submit());
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("Check your connection"),
    );
    expect(login).toHaveBeenCalledTimes(2);
  });

  describe("while the server wakes up", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    const settle = (ms: number) =>
      act(async () => {
        await vi.advanceTimersByTimeAsync(ms);
      });

    it.each([
      ["a network error", new ApiError(0, "NETWORK", "offline")],
      ["a 502", new ApiError(502, "UNKNOWN", "bad gateway")],
      ["a 503", new ApiError(503, "UNKNOWN", "unavailable")],
      ["a 504", new ApiError(504, "UNKNOWN", "timeout")],
    ])("tries again by itself after %s, says why, and signs in", async (_name, failure) => {
      const login = vi
        .spyOn(api.auth, "login")
        .mockRejectedValueOnce(failure)
        .mockRejectedValueOnce(failure)
        .mockResolvedValue(ada);
      renderWithSession(<LoginForm />);
      fill("ada@example.com", "secret");
      fireEvent.click(submit());
      await settle(0);
      expect(screen.getByRole("status")).toHaveTextContent("Waking up the server");
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
      expect(submit()).toBeDisabled();
      expect(login).toHaveBeenCalledTimes(1);
      await settle(WAKE_WAIT_MS);
      expect(login).toHaveBeenCalledTimes(2);
      await settle(WAKE_WAIT_MS);
      expect(login).toHaveBeenCalledTimes(3);
      expect(nav.replace).toHaveBeenCalledWith("/w");
    });

    it("gives up after the last attempt with a message that names the cause", async () => {
      const login = vi
        .spyOn(api.auth, "login")
        .mockRejectedValue(new ApiError(504, "UNKNOWN", "timeout"));
      renderWithSession(<LoginForm />);
      fill("ada@example.com", "secret");
      fireEvent.click(submit());
      await settle(WAKE_WAIT_MS * WAKE_ATTEMPTS);
      expect(login).toHaveBeenCalledTimes(WAKE_ATTEMPTS);
      expect(screen.getByRole("alert")).toHaveTextContent("may still be waking up");
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
      expect(submit()).toBeEnabled();
      expect(nav.replace).not.toHaveBeenCalled();
    });

    it("stops at once on a wrong password, even after waiting", async () => {
      const login = vi
        .spyOn(api.auth, "login")
        .mockRejectedValueOnce(new ApiError(504, "UNKNOWN", "timeout"))
        .mockRejectedValueOnce(new ApiError(401, "UNAUTHORIZED", "invalid email or password"));
      renderWithSession(<LoginForm />);
      fill("ada@example.com", "wrong");
      fireEvent.click(submit());
      await settle(WAKE_WAIT_MS);
      expect(login).toHaveBeenCalledTimes(2);
      expect(screen.getByRole("alert")).toHaveTextContent("Invalid email or password.");
    });

    it("does not keep trying, or navigate, after the page is left", async () => {
      const login = vi
        .spyOn(api.auth, "login")
        .mockRejectedValueOnce(new ApiError(0, "NETWORK", "offline"))
        .mockResolvedValue(ada);
      const view = renderWithSession(<LoginForm />);
      fill("ada@example.com", "secret");
      fireEvent.click(submit());
      await settle(0);
      view.unmount();
      await settle(WAKE_WAIT_MS * 3);
      expect(login).toHaveBeenCalledTimes(1);
      expect(nav.replace).not.toHaveBeenCalled();
    });
  });

  it("links to registration", () => {
    renderWithSession(<LoginForm />);
    expect(screen.getByRole("link", { name: "Create an account" })).toHaveAttribute(
      "href",
      "/register",
    );
  });

  describe("with GitHub", () => {
    it("offers Continue with GitHub only when the server has it set up", async () => {
      renderWithSession(<LoginForm />);
      await act(async () => {});
      expect(screen.queryByRole("link", { name: /Continue with GitHub/ })).not.toBeInTheDocument();
    });

    it("links the button to the start of the GitHub sign-in on the web domain", async () => {
      vi.spyOn(api.auth, "providers").mockResolvedValue({ github: true });
      renderWithSession(<LoginForm />);
      const link = await screen.findByRole("link", { name: /Continue with GitHub/ });
      expect(link).toHaveAttribute("href", "/api/v1/auth/github/start");
    });

    it("shows no button when the providers cannot be read", async () => {
      vi.spyOn(api.auth, "providers").mockRejectedValue(new ApiError(0, "NETWORK", "offline"));
      renderWithSession(<LoginForm />);
      await act(async () => {});
      expect(screen.queryByRole("link", { name: /Continue with GitHub/ })).not.toBeInTheDocument();
    });

    it.each(Object.entries(GITHUB_ERRORS))(
      "explains the callback error %s in words",
      (code, message) => {
        nav.query.value = `github_error=${code}`;
        renderWithSession(<LoginForm />);
        const alert = screen.getByRole("alert");
        expect(alert).toHaveTextContent("GitHub sign-in failed");
        expect(alert).toHaveTextContent(message);
      },
    );

    it("treats an unknown error code like a failure and shows nothing without one", () => {
      nav.query.value = "github_error=%3Cscript%3E";
      const view = renderWithSession(<LoginForm />);
      expect(screen.getByRole("alert")).toHaveTextContent(GITHUB_ERRORS.failed as string);
      expect(screen.getByRole("alert")).not.toHaveTextContent("script");
      view.unmount();
      nav.query.value = "";
      renderWithSession(<LoginForm />);
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });
});
