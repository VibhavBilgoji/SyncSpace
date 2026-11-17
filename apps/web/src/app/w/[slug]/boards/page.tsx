import { BoardsPage } from "@/components/boards/BoardsPage";
import { WorkspaceShell } from "@/components/shell/WorkspaceShell";

export const metadata = { title: "Boards · SyncSpace" };

export default function BoardsRoute() {
  return (
    <WorkspaceShell title="Boards">
      <BoardsPage />
    </WorkspaceShell>
  );
}
