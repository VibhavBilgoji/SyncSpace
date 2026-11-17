"use client";

import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { useToast } from "@/components/ui/Toast";

interface Props {
  open: boolean;
  onClose(): void;
  title: string;
  /** The URL members use to open the document. */
  link: string;
  workspaceName: string;
  memberCount: number;
}

/** Frame 15: the link is the document's own URL; only workspace members can open it. */
export function ShareDialog({ open, onClose, title, link, workspaceName, memberCount }: Props) {
  const { toast } = useToast();

  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
      toast({ title: "Link copied", tone: "success" });
    } catch {
      // No clipboard permission: the field is selectable, so say what to do instead.
      toast({
        title: "Could not copy",
        description: "Select the link and copy it.",
        tone: "warning",
      });
    }
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`Share ${title}`}
      description="Only workspace members can open this document."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Close
          </Button>
          <Button onClick={() => void copy()}>Copy link</Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Input label="Workspace link" value={link} readOnly onFocus={(e) => e.target.select()} />
        <p className="text-ui-sm text-text-muted">
          {memberCount} {memberCount === 1 ? "member" : "members"} in {workspaceName}.
        </p>
        <p className="text-ui-sm text-text-faint">
          Admins and editors can edit. Viewers can read and preview versions.
        </p>
      </div>
    </Dialog>
  );
}
