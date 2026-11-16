import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Documents · SyncSpace" };

/** Placeholder inside the shell; the documents list is task 2.5. */
export default function DocsPage() {
  return (
    <WorkspaceShell title="Documents">
      <h1 className="text-heading-lg">Documents</h1>
    </WorkspaceShell>
  );
}
