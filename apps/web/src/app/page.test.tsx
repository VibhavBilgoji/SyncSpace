import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import Home from "@/app/page";

describe("Home", () => {
  it("renders the SyncSpace placeholder", () => {
    render(<Home />);
    expect(screen.getByRole("heading", { name: "SyncSpace" })).toBeInTheDocument();
  });
});
