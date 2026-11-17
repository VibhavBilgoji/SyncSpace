import { expect, test, type Page } from "@playwright/test";
import { addMember, ownerWorkspace, signUp } from "./helpers";

// TESTING.md scenario 7, reduced (package D): a task is moved with the arrow buttons and the
// status dropdown, not by dragging. Create a board and tasks, move them, edit, delete; a viewer
// sees the board but cannot change it.
const column = (page: Page, label: string) => page.getByRole("region", { name: label });
const cards = (page: Page, label: string) =>
  column(page, label).getByRole("button", { name: /^Open task / });

test("create a board and tasks, move them with the buttons and the dropdown, delete one", async ({
  browser,
}) => {
  const { owner, workspaceId } = await ownerWorkspace(browser);
  const viewer = await signUp(browser, "viewer");
  await addMember(owner.context, workspaceId, viewer.email, "VIEWER");

  // A board from the boards page.
  await owner.page.getByRole("link", { name: "All boards" }).click();
  await expect(owner.page.getByText("No boards yet")).toBeVisible();
  await owner.page.getByRole("button", { name: "Create board" }).first().click();
  await owner.page.getByRole("dialog").getByLabel("Name").fill("Sprint 12");
  await owner.page.getByRole("dialog").getByRole("button", { name: "Create board" }).click();
  await owner.page.waitForURL(/\/boards\/[0-9a-f]{24}$/);
  const boardUrl = owner.page.url();
  await expect(owner.page.getByRole("heading", { name: "Sprint 12" })).toBeVisible();
  for (const label of ["Backlog", "To do", "In progress", "Review", "Done"]) {
    await expect(column(owner.page, label)).toBeVisible();
  }

  // Two tasks: one in Backlog, one in To do.
  await owner.page.getByRole("button", { name: "+ New Task" }).click();
  let dialog = owner.page.getByRole("dialog");
  await dialog.getByLabel("Title").fill("Write the plan");
  await dialog.getByLabel("Tags").fill("docs, planning");
  await dialog.getByLabel("Priority").selectOption("HIGH");
  await dialog.getByRole("button", { name: "Create task" }).click();
  await expect(cards(owner.page, "Backlog")).toHaveCount(1);
  await expect(owner.page.getByText("#docs")).toBeVisible();

  await owner.page.getByRole("button", { name: "Add task to To do" }).click();
  dialog = owner.page.getByRole("dialog");
  await dialog.getByLabel("Title").fill("Build the API");
  await dialog.getByRole("button", { name: "Create task" }).click();
  await expect(cards(owner.page, "To do")).toHaveCount(1);

  // The arrow moves "Write the plan" to the end of To do, behind "Build the API".
  await owner.page.getByRole("button", { name: "Move Write the plan to To do" }).click();
  await expect(cards(owner.page, "To do")).toHaveCount(2);
  await expect(cards(owner.page, "Backlog")).toHaveCount(0);
  await expect(cards(owner.page, "To do").nth(0)).toHaveAccessibleName("Open task Build the API");
  await expect(cards(owner.page, "To do").nth(1)).toHaveAccessibleName("Open task Write the plan");

  // The server agrees on the order, and the move is permanent after a reload.
  const boardId = boardUrl.split("/").pop()!;
  const listed = await owner.context.request.get(`/api/v1/boards/${boardId}/tasks`);
  const tasks = (await listed.json()) as Array<{ title: string; status: string; order: number }>;
  expect(tasks.map((t) => [t.title, t.status])).toEqual([
    ["Build the API", "TODO"],
    ["Write the plan", "TODO"],
  ]);
  expect(tasks[0]!.order).toBeLessThan(tasks[1]!.order);
  await owner.page.reload();
  await expect(cards(owner.page, "To do")).toHaveCount(2);

  // Edit: the status dropdown moves the task to Review, and the fields are saved.
  await owner.page.getByRole("button", { name: "Open task Build the API" }).click();
  dialog = owner.page.getByRole("dialog");
  await dialog.getByLabel("Title").fill("Build the REST API");
  await dialog.getByLabel("Status").selectOption("REVIEW");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(cards(owner.page, "Review")).toHaveCount(1);
  await expect(cards(owner.page, "Review").first()).toHaveAccessibleName(
    "Open task Build the REST API",
  );
  await expect(cards(owner.page, "To do")).toHaveCount(1);

  // A viewer sees the board and cannot change it.
  await viewer.page.goto(boardUrl);
  await expect(viewer.page.getByRole("heading", { name: "Sprint 12" })).toBeVisible();
  await expect(cards(viewer.page, "Review")).toHaveCount(1);
  await expect(viewer.page.getByRole("button", { name: "+ New Task" })).toHaveCount(0);
  await expect(viewer.page.getByRole("button", { name: /^Move / })).toHaveCount(0);
  await viewer.page.getByRole("button", { name: "Open task Build the REST API" }).click();
  await expect(viewer.page.getByRole("dialog").getByRole("button", { name: "Save" })).toHaveCount(0);
  await expect(viewer.page.getByRole("dialog").getByRole("button", { name: "Delete" })).toHaveCount(0);
  await viewer.page.getByRole("dialog").getByRole("button", { name: "Close" }).click();

  // The viewer picks up the owner's next change when the window gets focus again.
  await owner.page.getByRole("button", { name: "Move Build the REST API to Done" }).click();
  await expect(cards(owner.page, "Done")).toHaveCount(1);
  await viewer.page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(cards(viewer.page, "Done")).toHaveCount(1);
  await expect(cards(viewer.page, "Review")).toHaveCount(0);

  // Delete asks first, then removes the task for everyone.
  await owner.page.getByRole("button", { name: "Open task Write the plan" }).click();
  await owner.page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await expect(owner.page.getByText("Delete task?")).toBeVisible();
  await owner.page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click();
  await owner.page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await owner.page.getByRole("dialog").getByRole("button", { name: "Delete task" }).click();
  await expect(cards(owner.page, "To do")).toHaveCount(0);
  await expect(owner.page.getByRole("button", { name: "Open task Write the plan" })).toHaveCount(0);

  await owner.context.close();
  await viewer.context.close();
});
