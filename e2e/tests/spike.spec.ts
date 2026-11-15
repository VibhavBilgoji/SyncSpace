import { expect, test, type Page } from "@playwright/test";

// THROWAWAY (roadmap task 0.3): two browser tabs, each running the Wasm CRDT, converge
// through the spike relay. Replaced by the real collaboration tests in Phase 2.

const text = (page: Page) => page.getByTestId("spike-text");

async function openSpike(page: Page) {
  await page.goto("/spike");
  // "connected" only means the socket opened. Wait until the relay's full state has been
  // applied too, or clearing the shared text can race with that first frame.
  const status = page.getByTestId("spike-status");
  await expect(status).toHaveText("connected");
  await expect(status).toHaveAttribute("data-synced", "true");
}

test("two tabs converge", async ({ browser }) => {
  // Separate contexts: separate storage and sockets, like two different people.
  const contextA = await browser.newContext();
  const contextB = await browser.newContext();
  const a = await contextA.newPage();
  const b = await contextB.newPage();

  try {
    await Promise.all([openSpike(a), openSpike(b)]);

    // The relay keeps its document between runs, so start from an empty text.
    await text(a).fill("");
    await expect(text(a)).toHaveValue("");
    await expect(text(b)).toHaveValue("");

    // One tab types, the other shows it.
    await text(a).pressSequentially("hello");
    await expect(text(b)).toHaveValue("hello");

    // Both type at once. A types at the end; B's caret is at the start, so the two
    // edits do not interleave and the caret mapping on remote updates is exercised too.
    await Promise.all([
      text(a).pressSequentially(" from A"),
      text(b).pressSequentially("B says: "),
    ]);

    await expect
      .poll(async () => (await text(a).inputValue()) === (await text(b).inputValue()))
      .toBe(true);
    const merged = await text(a).inputValue();
    expect(merged).toContain("hello from A");
    expect(merged).toContain("B says: ");
    expect(merged).toHaveLength("hello from A".length + "B says: ".length);

    // A tab that opens later receives the full state from the relay.
    await b.reload();
    await expect(b.getByTestId("spike-status")).toHaveAttribute("data-synced", "true");
    await expect(text(b)).toHaveValue(merged);
  } finally {
    await contextA.close();
    await contextB.close();
  }
});

test("emoji and non-Latin text survive the round trip", async ({ browser }) => {
  const contextA = await browser.newContext();
  const contextB = await browser.newContext();
  const a = await contextA.newPage();
  const b = await contextB.newPage();

  try {
    await Promise.all([openSpike(a), openSpike(b)]);
    await text(a).fill("");
    await expect(text(b)).toHaveValue("");

    await text(a).fill("hi 🙂 नमस्ते");
    await expect(text(b)).toHaveValue("hi 🙂 नमस्ते");

    // Replacing the emoji with another one that shares its first UTF-16 unit.
    await text(a).fill("hi 😀 नमस्ते");
    await expect(text(b)).toHaveValue("hi 😀 नमस्ते");
  } finally {
    await contextA.close();
    await contextB.close();
  }
});
