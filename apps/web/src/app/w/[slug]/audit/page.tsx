import { AuditPage } from "@/components/audit/AuditPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Audit trail · SyncSpace" };

export default function AuditRoute() {
  return (
    <WorkspaceShell title="Audit Trail">
      <AuditPage />
    </WorkspaceShell>
  );
}
