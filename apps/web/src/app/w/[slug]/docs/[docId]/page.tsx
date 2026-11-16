import { DocumentPage } from "@/components/editor/DocumentPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Document · SyncSpace" };

export default function DocPage() {
  return (
    <WorkspaceShell title="Document">
      <DocumentPage />
    </WorkspaceShell>
  );
}
