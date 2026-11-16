import { DocumentsPage } from "@/components/documents/DocumentsPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Documents · SyncSpace" };

export default function DocsPage() {
  return (
    <WorkspaceShell title="Documents">
      <DocumentsPage />
    </WorkspaceShell>
  );
}
