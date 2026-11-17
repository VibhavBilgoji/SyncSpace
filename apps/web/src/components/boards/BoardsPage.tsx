"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState, type FormEvent } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { api, isApiError } from "@/lib/api";
import type { Board } from "@/types/generated";

type State = { status: "loading" } | { status: "error" } | { status: "ready"; boards: Board[] };

const total = (board: Board) =>
  board.task_counts.backlog +
  board.task_counts.todo +
  board.task_counts.in_progress +
  board.task_counts.review +
  board.task_counts.done;

/** Frames 27a (create board) and 27b (empty): the boards of the workspace. */
export function BoardsPage() {
  const workspace = useWorkspace();
  const router = useRouter();
  const [state, setState] = useState<State>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [creating, setCreating] = useState(false);
  const canCreate = workspace.role !== "VIEWER";

  useEffect(() => {
    let cancelled = false;
    api.boards
      .list(workspace.id)
      .then((boards) => !cancelled && setState({ status: "ready", boards }))
      .catch(() => !cancelled && setState({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [workspace.id, attempt]);

  const createButton = canCreate ? (
    <Button onClick={() => setCreating(true)}>Create board</Button>
  ) : undefined;

  return (
    <div className="mx-auto max-w-3xl">
      <div className="mb-6 flex items-center justify-between gap-4">
        <h1 className="text-heading-lg">Boards</h1>
        {state.status === "ready" && state.boards.length > 0 && createButton}
      </div>

      {state.status === "loading" && (
        <div aria-busy="true" aria-label="Loading boards" className="flex flex-col gap-2">
          {[0, 1].map((n) => (
            <div key={n} className="h-14 animate-pulse rounded-card bg-surface" />
          ))}
        </div>
      )}

      {state.status === "error" && (
        <Banner
          tone="danger"
          title="Could not load the boards."
          action={
            <Button
              variant="secondary"
              onClick={() => {
                setState({ status: "loading" });
                setAttempt((n) => n + 1);
              }}
            >
              Retry
            </Button>
          }
        />
      )}

      {state.status === "ready" && state.boards.length === 0 && (
        <EmptyState
          title="No boards yet"
          description={
            canCreate
              ? "Create a board to plan work in five columns."
              : "Boards created by editors will appear here."
          }
          action={createButton}
        />
      )}

      {state.status === "ready" && state.boards.length > 0 && (
        <ul className="flex flex-col gap-2">
          {state.boards.map((board) => (
            <li key={board.id}>
              <Link
                href={`/w/${workspace.slug}/boards/${board.id}`}
                className="flex items-center gap-3 rounded-card border border-border bg-surface px-4 py-3 hover:bg-hover"
              >
                <span className="min-w-0 flex-1 truncate text-ui-base">{board.name}</span>
                <span className="text-micro text-text-muted">
                  {total(board)} {total(board) === 1 ? "task" : "tasks"}
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}

      {creating && (
        <CreateBoardDialog
          workspaceId={workspace.id}
          onClose={() => setCreating(false)}
          onCreated={(board) => {
            setCreating(false);
            router.push(`/w/${workspace.slug}/boards/${board.id}`);
          }}
        />
      )}
    </div>
  );
}

function CreateBoardDialog({
  workspaceId,
  onClose,
  onCreated,
}: {
  workspaceId: string;
  onClose(): void;
  onCreated(board: Board): void;
}) {
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | undefined>();
  const [formError, setFormError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const valid = name.trim() !== "";

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setNameError(undefined);
    setFormError(null);
    try {
      onCreated(await api.boards.create(workspaceId, { name: name.trim() }));
    } catch (e) {
      if (isApiError(e) && e.status === 422) setNameError(e.details.name);
      else if (isApiError(e) && e.status === 403)
        setFormError("Your role in this workspace does not allow creating boards.");
      else setFormError("Could not create the board. Check your connection and try again.");
    } finally {
      setLoading(false);
    }
  }

  return (
    <Dialog
      open
      onClose={onClose}
      title="Create board"
      description="A board has five columns: Backlog, To do, In progress, Review and Done."
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        <Input
          label="Name"
          maxLength={60}
          value={name}
          error={nameError}
          onChange={(e) => setName(e.target.value)}
        />
        {formError && <Banner tone="danger">{formError}</Banner>}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!valid} loading={loading}>
            Create board
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
