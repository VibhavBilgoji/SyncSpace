import { RepositoriesPage } from "@/components/github/RepositoriesPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Repositories · SyncSpace" };

export default function RepositoriesRoute() {
  return (
    <WorkspaceShell title="Repositories">
      <RepositoriesPage />
    </WorkspaceShell>
  );
}
