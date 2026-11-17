import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HeaderActions, HeaderSlot, HeaderSlotProvider } from "./headerSlot";

describe("header slot", () => {
  it("shows a page's controls inside the slot, wherever the page sits in the tree", () => {
    const { container } = render(
      <HeaderSlotProvider>
        <header>
          <HeaderSlot />
        </header>
        <main>
          <HeaderActions>
            <button>From the page</button>
          </HeaderActions>
        </main>
      </HeaderSlotProvider>,
    );
    const slot = container.querySelector("[data-header-slot]");
    expect(slot).toContainElement(screen.getByRole("button", { name: "From the page" }));
    expect(container.querySelector("main")).not.toContainElement(
      screen.getByRole("button", { name: "From the page" }),
    );
  });

  it("removes the controls when the page goes away", () => {
    const tree = (show: boolean) => (
      <HeaderSlotProvider>
        <HeaderSlot />
        {show && (
          <HeaderActions>
            <button>From the page</button>
          </HeaderActions>
        )}
      </HeaderSlotProvider>
    );
    const view = render(tree(true));
    expect(screen.getByRole("button", { name: "From the page" })).toBeInTheDocument();
    view.rerender(tree(false));
    expect(screen.queryByRole("button", { name: "From the page" })).not.toBeInTheDocument();
  });

  it("renders nothing, and does not fail, outside a header", () => {
    render(
      <HeaderActions>
        <button>Orphan</button>
      </HeaderActions>,
    );
    expect(screen.queryByRole("button", { name: "Orphan" })).not.toBeInTheDocument();
  });
});
