import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Avatar, AvatarStack } from "./Avatar";
import { Badge } from "./Badge";
import { Banner } from "./Banner";
import { EmptyState } from "./EmptyState";
import { NavItem } from "./NavItem";

describe("Badge", () => {
  it("renders its text with the chosen tone and defaults to muted", () => {
    const { rerender } = render(<Badge tone="violet">Restore</Badge>);
    expect(screen.getByText("Restore")).toHaveAttribute("data-tone", "violet");
    rerender(<Badge>Automatic</Badge>);
    expect(screen.getByText("Automatic")).toHaveAttribute("data-tone", "muted");
  });
});

describe("Avatar", () => {
  it("shows the upper-cased first letter and is labelled with the name", () => {
    render(<Avatar name="ada lovelace" />);
    const avatar = screen.getByRole("img", { name: "ada lovelace" });
    expect(avatar).toHaveTextContent("A");
  });

  it("falls back to ? for an empty name and gives one name one colour", () => {
    const { container } = render(
      <>
        <Avatar name="  " />
        <Avatar name="Grace" />
        <Avatar name="Grace" />
      </>,
    );
    const [blank, a, b] = Array.from(container.children);
    expect(blank).toHaveTextContent("?");
    expect(a?.className).toBe(b?.className);
  });
});

describe("AvatarStack", () => {
  it("shows at most three avatars and a +N counter", () => {
    render(<AvatarStack names={["Ada", "Grace", "Linus", "Margaret", "Dennis"]} />);
    expect(screen.getAllByRole("img")).toHaveLength(4);
    expect(screen.getByRole("img", { name: "2 more" })).toHaveTextContent("+2");
  });

  it("has no counter when everyone fits", () => {
    render(<AvatarStack names={["Ada", "Grace"]} />);
    expect(screen.getAllByRole("img")).toHaveLength(2);
    expect(screen.queryByText(/^\+/)).not.toBeInTheDocument();
  });
});

describe("NavItem", () => {
  it("is a link, marks the current page and shows the trailing slot", () => {
    render(
      <NavItem href="/w/eng/audit" active trailing={<Badge tone="success">Admin</Badge>}>
        Audit Trail
      </NavItem>,
    );
    const link = screen.getByRole("link", { name: /Audit Trail/ });
    expect(link).toHaveAttribute("href", "/w/eng/audit");
    expect(link).toHaveAttribute("aria-current", "page");
    expect(link).toHaveTextContent("Admin");
  });

  it("is not current by default", () => {
    render(<NavItem href="/w/eng/docs">Documents</NavItem>);
    expect(screen.getByRole("link")).not.toHaveAttribute("aria-current");
  });
});

describe("Banner", () => {
  it("uses status for info and warning and alert for danger", () => {
    const { rerender } = render(<Banner tone="warning" title="Reconnecting..." />);
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting...");
    rerender(
      <Banner tone="danger" title="Error">
        Something failed
      </Banner>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Something failed");
  });

  it("renders an action", () => {
    render(<Banner action={<button>Retry</button>}>Offline</Banner>);
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});

describe("EmptyState", () => {
  it("shows title, description and the action when given", () => {
    render(
      <EmptyState
        title="No documents yet"
        description="Create one."
        action={<button>Create document</button>}
      />,
    );
    expect(screen.getByRole("heading", { name: "No documents yet" })).toBeInTheDocument();
    expect(screen.getByText("Create one.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create document" })).toBeInTheDocument();
  });

  it("hides the action for roles that cannot use it", () => {
    render(<EmptyState title="No documents yet" />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
