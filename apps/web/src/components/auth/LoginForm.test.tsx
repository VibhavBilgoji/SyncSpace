import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { ada, renderWithSession } from "@/test/render";
import { LoginForm } from "./LoginForm";

const nav = vi.hoisted(() => ({ replace: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => nav }));

beforeEach(() => nav.replace.mockClear());

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

  it("explains a rate limit and a network failure differently", async () => {
    const login = vi
      .spyOn(api.auth, "login")
      .mockRejectedValueOnce(new ApiError(429, "RATE_LIMITED", "slow down"))
      .mockRejectedValueOnce(new ApiError(0, "NETWORK", "offline"));
    renderWithSession(<LoginForm />);
    fill("ada@example.com", "secret");
    fireEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent("Too many attempts");
    fireEvent.click(submit());
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Could not sign in"));
    expect(login).toHaveBeenCalledTimes(2);
  });

  it("links to registration", () => {
    renderWithSession(<LoginForm />);
    expect(screen.getByRole("link", { name: "Create an account" })).toHaveAttribute(
      "href",
      "/register",
    );
  });
});
