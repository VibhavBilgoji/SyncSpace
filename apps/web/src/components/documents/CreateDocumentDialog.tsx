"use client";

import { useState, type FormEvent } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { api, isApiError } from "@/lib/api";
import type { Document } from "@/types/generated";

interface Props {
  open: boolean;
  workspaceId: string;
  onClose: () => void;
  onCreated: (document: Document) => void;
}

/** Frame 08. A title is required; the icon is optional and defaults on the server. */
export function CreateDocumentDialog({ open, workspaceId, onClose, onCreated }: Props) {
  const [title, setTitle] = useState("");
  const [icon, setIcon] = useState("");
  const [loading, setLoading] = useState(false);
  const [titleError, setTitleError] = useState<string | undefined>();
  const [iconError, setIconError] = useState<string | undefined>();
  const [formError, setFormError] = useState<string | null>(null);

  const valid = title.trim() !== "";

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setTitleError(undefined);
    setIconError(undefined);
    setFormError(null);
    try {
      const created = await api.documents.create(workspaceId, {
        title: title.trim(),
        ...(icon.trim() ? { icon: icon.trim() } : {}),
      });
      setTitle("");
      setIcon("");
      onCreated(created);
    } catch (e) {
      if (isApiError(e) && e.status === 422) {
        setTitleError(e.details.title);
        setIconError(e.details.icon);
      } else if (isApiError(e) && e.status === 403) {
        setFormError("Your role in this workspace does not allow creating documents.");
      } else {
        setFormError("Could not create the document. Check your connection and try again.");
      }
    } finally {
      setLoading(false);
    }
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Create document"
      description="A new document starts with one empty paragraph."
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        <Input
          label="Title"
          maxLength={120}
          value={title}
          error={titleError}
          onChange={(e) => setTitle(e.target.value)}
        />
        <Input
          label="Icon (optional)"
          maxLength={16}
          value={icon}
          hint="One emoji or a short key. Leave empty for the default."
          error={iconError}
          onChange={(e) => setIcon(e.target.value)}
        />
        {formError && <Banner tone="danger">{formError}</Banner>}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!valid} loading={loading}>
            Create document
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
