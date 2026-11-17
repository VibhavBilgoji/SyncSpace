import { BoardPage } from "@/components/boards/BoardPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Board · SyncSpace" };

export default function BoardRoute() {
  return (
    <WorkspaceShell title="Board">
      <BoardPage />
    </WorkspaceShell>
  );
}
