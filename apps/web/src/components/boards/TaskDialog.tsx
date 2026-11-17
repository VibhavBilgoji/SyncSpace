"use client";

import { useState, type FormEvent } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { api, isApiError } from "@/lib/api";
import type { Member, Priority, Status, Task } from "@/types/generated";
import { COLUMNS, PRIORITIES, PRIORITY_LABEL } from "./columns";

export type TaskDialogMode = { kind: "create"; status: Status } | { kind: "edit"; task: Task };

interface Props {
  mode: TaskDialogMode;
  boardId: string;
  members: Member[];
  /** Viewers see the details and cannot change or delete anything (frame 31). */
  canEdit: boolean;
  onClose(): void;
  /** A task was created or changed: the final state, after any move. */
  onSaved(task: Task): void;
  onDeleted(taskId: string): void;
}

const FIELD_ERRORS = ["title", "description", "assignee_ids", "tags"] as const;

/** Frames 28 (create), 29 (details and edit), 31 (viewer) and 32 (delete). */
export function TaskDialog({
  mode,
  boardId,
  members,
  canEdit,
  onClose,
  onSaved,
  onDeleted,
}: Props) {
  const task = mode.kind === "edit" ? mode.task : null;
  const [title, setTitle] = useState(task?.title ?? "");
  const [description, setDescription] = useState(task?.description ?? "");
  const [status, setStatus] = useState<Status>(
    task?.status ?? (mode.kind === "create" ? mode.status : "BACKLOG"),
  );
  const [priority, setPriority] = useState<Priority>(task?.priority ?? "MEDIUM");
  const [assignees, setAssignees] = useState<string[]>(task?.assignee_ids ?? []);
  const [tags, setTags] = useState((task?.tags ?? []).join(", "));
  const [errors, setErrors] = useState<Partial<Record<(typeof FIELD_ERRORS)[number], string>>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const valid = title.trim() !== "";
  const tagList = tags
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);

  function toggleAssignee(userId: string) {
    setAssignees((current) =>
      current.includes(userId) ? current.filter((id) => id !== userId) : [...current, userId],
    );
  }

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!canEdit || !valid || saving) return;
    setSaving(true);
    setErrors({});
    setFormError(null);
    try {
      let saved: Task;
      if (mode.kind === "create") {
        saved = await api.boards.createTask(boardId, {
          title: title.trim(),
          description,
          status,
          priority,
          assignee_ids: assignees,
          tags: tagList,
        });
      } else {
        saved = await api.boards.updateTask(mode.task.id, {
          title: title.trim(),
          description,
          priority,
          assignee_ids: assignees,
          tags: tagList,
        });
        // The column is changed by a move, which also puts the task at the end of that column.
        if (status !== mode.task.status) {
          saved = await api.boards.moveTask(mode.task.id, { to_status: status });
        }
      }
      onSaved(saved);
    } catch (e) {
      if (isApiError(e) && e.status === 422) {
        const next: typeof errors = {};
        for (const field of FIELD_ERRORS) next[field] = e.details[field];
        setErrors(next);
      } else if (isApiError(e) && e.status === 403) {
        setFormError("Your role in this workspace does not allow changing tasks.");
      } else if (isApiError(e) && e.status === 404) {
        setFormError("This task no longer exists. Close this window to refresh the board.");
      } else {
        setFormError("Could not save the task. Check your connection and try again.");
      }
    } finally {
      setSaving(false);
    }
  }

  async function confirmDelete() {
    if (!task) return;
    setDeleting(true);
    try {
      await api.boards.removeTask(task.id);
      onDeleted(task.id);
    } catch (e) {
      setConfirmingDelete(false);
      setFormError(
        isApiError(e) && e.status === 404
          ? "This task was already deleted. Close this window to refresh the board."
          : "Could not delete the task. Check your connection and try again.",
      );
    } finally {
      setDeleting(false);
    }
  }

  if (confirmingDelete && task) {
    return (
      <Dialog
        open
        onClose={() => setConfirmingDelete(false)}
        title="Delete task?"
        description={`"${task.title}" will be deleted for everyone. This cannot be undone.`}
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirmingDelete(false)}>
              Cancel
            </Button>
            <Button variant="danger" loading={deleting} onClick={() => void confirmDelete()}>
              Delete task
            </Button>
          </>
        }
      />
    );
  }

  return (
    <Dialog
      open
      onClose={onClose}
      title={mode.kind === "create" ? "New task" : canEdit ? "Edit task" : "Task details"}
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        <Input
          label="Title"
          maxLength={200}
          value={title}
          error={errors.title}
          readOnly={!canEdit}
          onChange={(e) => setTitle(e.target.value)}
        />
        <div className="flex flex-col gap-1.5">
          <label htmlFor="task-description" className="text-ui-sm text-text-muted">
            Description
          </label>
          <textarea
            id="task-description"
            rows={3}
            maxLength={5000}
            value={description}
            readOnly={!canEdit}
            onChange={(e) => setDescription(e.target.value)}
            aria-invalid={errors.description ? true : undefined}
            className="w-full rounded-control border border-border bg-surface px-3 py-2 text-ui-base text-text"
          />
          {errors.description && (
            <p role="alert" className="text-ui-sm text-danger">
              {errors.description}
            </p>
          )}
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Select
            label="Status"
            value={status}
            disabled={!canEdit}
            options={COLUMNS.map((c) => ({ value: c.status, label: c.label }))}
            onChange={(e) => setStatus(e.target.value as Status)}
          />
          <Select
            label="Priority"
            value={priority}
            disabled={!canEdit}
            options={PRIORITIES.map((p) => ({ value: p, label: PRIORITY_LABEL[p] }))}
            onChange={(e) => setPriority(e.target.value as Priority)}
          />
        </div>
        <fieldset className="flex flex-col gap-1.5">
          <legend className="text-ui-sm text-text-muted">Assignees</legend>
          <div className="flex flex-wrap gap-x-4 gap-y-1">
            {members.map((member) => (
              <label key={member.user_id} className="flex items-center gap-2 text-ui-sm">
                <input
                  type="checkbox"
                  checked={assignees.includes(member.user_id)}
                  disabled={!canEdit}
                  onChange={() => toggleAssignee(member.user_id)}
                />
                {member.name}
              </label>
            ))}
          </div>
          {errors.assignee_ids && (
            <p role="alert" className="text-ui-sm text-danger">
              {errors.assignee_ids}
            </p>
          )}
        </fieldset>
        <Input
          label="Tags"
          value={tags}
          hint="Comma separated, for example backend, api"
          error={errors.tags}
          readOnly={!canEdit}
          onChange={(e) => setTags(e.target.value)}
        />
        {formError && <Banner tone="danger">{formError}</Banner>}
        <div className="flex items-center justify-between gap-2">
          <div>
            {canEdit && task && (
              <Button type="button" variant="danger" onClick={() => setConfirmingDelete(true)}>
                Delete
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="secondary" onClick={onClose}>
              {canEdit ? "Cancel" : "Close"}
            </Button>
            {canEdit && (
              <Button type="submit" disabled={!valid} loading={saving}>
                {mode.kind === "create" ? "Create task" : "Save"}
              </Button>
            )}
          </div>
        </div>
      </form>
    </Dialog>
  );
}
