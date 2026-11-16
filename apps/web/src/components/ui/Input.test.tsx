import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Input } from "./Input";
import { Select } from "./Select";

describe("Input", () => {
  it("links the label to the input and reports changes", () => {
    const onChange = vi.fn();
    render(<Input label="Email" onChange={onChange} />);
    const input = screen.getByLabelText("Email");
    fireEvent.change(input, { target: { value: "a@b.co" } });
    expect(onChange).toHaveBeenCalled();
  });

  it("shows the hint and wires aria-describedby", () => {
    render(<Input label="Slug" hint="Lowercase letters and dashes" />);
    const input = screen.getByLabelText("Slug");
    expect(input).not.toHaveAttribute("aria-invalid");
    expect(input).toHaveAccessibleDescription("Lowercase letters and dashes");
  });

  it("marks the input invalid and announces the error", () => {
    render(<Input label="Password" error="Too short" hint="ignored when there is an error" />);
    const input = screen.getByLabelText("Password");
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("Too short");
    expect(screen.getByRole("alert")).toHaveTextContent("Too short");
  });

  it("can be disabled", () => {
    render(<Input label="Name" disabled />);
    expect(screen.getByLabelText("Name")).toBeDisabled();
  });
});

describe("Select", () => {
  const options = [
    { value: "ADMIN", label: "Admin" },
    { value: "VIEWER", label: "Viewer" },
  ];

  it("lists the options and reports the chosen value", () => {
    const onChange = vi.fn();
    render(<Select label="Role" options={options} defaultValue="ADMIN" onChange={onChange} />);
    const select = screen.getByLabelText("Role") as HTMLSelectElement;
    expect(select.value).toBe("ADMIN");
    fireEvent.change(select, { target: { value: "VIEWER" } });
    expect(onChange).toHaveBeenCalled();
    expect(select.value).toBe("VIEWER");
  });

  it("shows an error and can be disabled", () => {
    render(<Select label="Role" options={options} error="Pick a role" disabled />);
    const select = screen.getByLabelText("Role");
    expect(select).toBeDisabled();
    expect(select).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Pick a role");
  });
});
