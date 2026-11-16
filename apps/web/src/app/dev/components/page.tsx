import { Gallery } from "./Gallery";

export const metadata = { title: "Components · SyncSpace" };

/** Visual check of the UI kit (task 1.5). Not linked from the app. */
export default function ComponentsPage() {
  return (
    <main className="mx-auto flex max-w-4xl flex-col gap-8 p-8">
      <h1 className="text-heading-lg">Components</h1>
      <Gallery />
    </main>
  );
}
