import { MembersPage } from "@/components/settings/MembersPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Members · SyncSpace" };

export default function MembersRoute() {
  return (
    <WorkspaceShell title="Settings & Members">
      <MembersPage />
    </WorkspaceShell>
  );
}
