"use client";

import { useState, type ReactNode } from "react";
import { Avatar, AvatarStack } from "@/components/ui/Avatar";
import { Badge } from "@/components/ui/Badge";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { EmptyState } from "@/components/ui/EmptyState";
import { Input } from "@/components/ui/Input";
import { NavItem } from "@/components/ui/NavItem";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-micro text-text-muted uppercase">{title}</h2>
      <div className="flex flex-wrap items-start gap-3">{children}</div>
    </section>
  );
}

const roles = [
  { value: "ADMIN", label: "Admin" },
  { value: "EDITOR", label: "Editor" },
  { value: "VIEWER", label: "Viewer" },
];

export function Gallery() {
  const [open, setOpen] = useState(false);
  const { toast } = useToast();

  return (
    <div className="flex flex-col gap-8">
      <Section title="Button">
        <Button>Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="danger">Danger</Button>
        <Button variant="ghost">Ghost</Button>
        <Button disabled>Disabled</Button>
        <Button loading>Loading</Button>
      </Section>

      <Section title="Input and Select">
        <div className="w-64">
          <Input label="Email" placeholder="you@example.com" hint="We never share it." />
        </div>
        <div className="w-64">
          <Input label="Password" type="password" error="Password must be 8 to 128 characters" />
        </div>
        <div className="w-64">
          <Input label="Disabled" disabled defaultValue="Read only" />
        </div>
        <div className="w-64">
          <Select label="Role" options={roles} defaultValue="EDITOR" />
        </div>
      </Section>

      <Section title="Badge">
        <Badge tone="danger">Urgent</Badge>
        <Badge tone="warning">High</Badge>
        <Badge tone="indigo">Manual</Badge>
        <Badge tone="violet">Restore</Badge>
        <Badge tone="success">Admin</Badge>
        <Badge tone="muted">Automatic</Badge>
      </Section>

      <Section title="Avatar and AvatarStack">
        <Avatar name="Ada Lovelace" />
        <Avatar name="Grace Hopper" size="sm" />
        <AvatarStack names={["Ada", "Grace", "Linus", "Margaret", "Dennis"]} />
      </Section>

      <Section title="NavItem">
        <nav className="w-60 rounded-card bg-surface p-2">
          <NavItem href="/dev/components" active>
            Product Spec
          </NavItem>
          <NavItem href="/dev/components" trailing={<Badge tone="success">Admin</Badge>}>
            Audit Trail
          </NavItem>
        </nav>
      </Section>

      <Section title="Banner">
        <div className="flex w-full max-w-2xl flex-col gap-2">
          <Banner tone="info" title="VIEWER">
            Read-only access. You can follow edits and preview saved versions.
          </Banner>
          <Banner tone="warning" title="Reconnecting...">
            Your edits are kept in this tab and will merge when connected.
          </Banner>
          <Banner tone="danger" title="Sign-in failed">
            Invalid email or password.
          </Banner>
        </div>
      </Section>

      <Section title="EmptyState">
        <div className="w-full max-w-2xl">
          <EmptyState
            title="No documents yet"
            description="Create the first document in this workspace."
            action={<Button>Create document</Button>}
          />
        </div>
      </Section>

      <Section title="Dialog and Toast">
        <Button variant="secondary" onClick={() => setOpen(true)}>
          Open dialog
        </Button>
        <Button
          variant="secondary"
          onClick={() =>
            toast({ title: "Saved", description: "Version 4 created.", tone: "success" })
          }
        >
          Success toast
        </Button>
        <Button
          variant="secondary"
          onClick={() =>
            toast({
              title: "Your role was changed to VIEWER",
              description: "Write capabilities have been revoked.",
              tone: "danger",
            })
          }
        >
          Danger toast
        </Button>
        <Dialog
          open={open}
          onClose={() => setOpen(false)}
          title="Create named version"
          description="Give this snapshot a label so you can find it later."
          footer={
            <>
              <Button variant="ghost" onClick={() => setOpen(false)}>
                Cancel
              </Button>
              <Button onClick={() => setOpen(false)}>Create</Button>
            </>
          }
        >
          <Input label="Label" maxLength={80} />
        </Dialog>
      </Section>
    </div>
  );
}
