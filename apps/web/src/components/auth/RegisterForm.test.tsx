import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api } from "@/lib/api";
import { ada, renderWithSession } from "@/test/render";
import { RegisterForm } from "./RegisterForm";

const nav = vi.hoisted(() => ({ replace: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => nav }));

beforeEach(() => nav.replace.mockClear());

function fill(name: string, email: string, password: string) {
  fireEvent.change(screen.getByLabelText("Name"), { target: { value: name } });
  fireEvent.change(screen.getByLabelText("Email"), { target: { value: email } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: password } });
}

const submit = () => screen.getByRole("button", { name: "Create account" });

describe("RegisterForm", () => {
  it("needs a name, an email and a password of at least 8 characters", () => {
    renderWithSession(<RegisterForm />);
    expect(submit()).toBeDisabled();
    fill("Ada", "ada@example.com", "short");
    expect(submit()).toBeDisabled();
    fill("Ada", "ada@example.com", "long enough");
    expect(submit()).toBeEnabled();
  });

  it("registers and goes to the picker", async () => {
    const register = vi.spyOn(api.auth, "register").mockResolvedValue(ada);
    renderWithSession(<RegisterForm />);
    fill(" Ada ", "ada@example.com", "long enough");
    fireEvent.click(submit());
    await waitFor(() => expect(nav.replace).toHaveBeenCalledWith("/w"));
    expect(register).toHaveBeenCalledWith({
      name: "Ada",
      email: "ada@example.com",
      password: "long enough",
    });
  });

  it("shows a taken email under the email field", async () => {
    vi.spyOn(api.auth, "register").mockRejectedValue(new ApiError(409, "CONFLICT", "taken"));
    renderWithSession(<RegisterForm />);
    fill("Ada", "ada@example.com", "long enough");
    fireEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent("already exists");
    expect(screen.getByLabelText("Email")).toHaveAttribute("aria-invalid", "true");
    expect(nav.replace).not.toHaveBeenCalled();
  });

  it("maps 422 details to their fields", async () => {
    vi.spyOn(api.auth, "register").mockRejectedValue(
      new ApiError(422, "VALIDATION", "invalid", {
        name: "name has control characters",
        password: "password is too common",
      }),
    );
    renderWithSession(<RegisterForm />);
    fill("Ada", "ada@example.com", "long enough");
    fireEvent.click(submit());
    expect(await screen.findByText("name has control characters")).toBeInTheDocument();
    expect(screen.getByText("password is too common")).toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
  });

  it("shows a general error for anything else", async () => {
    vi.spyOn(api.auth, "register").mockRejectedValue(new ApiError(500, "INTERNAL", "boom"));
    renderWithSession(<RegisterForm />);
    fill("Ada", "ada@example.com", "long enough");
    fireEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not create the account");
  });
});
