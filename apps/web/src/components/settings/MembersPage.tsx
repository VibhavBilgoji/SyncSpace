"use client";

import { useEffect, useState, type FormEvent } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Avatar } from "@/components/ui/Avatar";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { Input } from "@/components/ui/Input";
import { Select } from "@/components/ui/Select";
import { useToast } from "@/components/ui/Toast";
import { api, isApiError } from "@/lib/api";
import { useSession } from "@/lib/session";
import type { Member, Role } from "@/types/generated";

type State = { status: "loading" } | { status: "error" } | { status: "ready"; members: Member[] };

const ROLES: Role[] = ["VIEWER", "EDITOR", "ADMIN"];

/** What each role may do, shown when a role is changed (frame 36). */
const ROLE_TEXT: Record<Role, string> = {
  VIEWER: "Viewer: reads documents and boards and previews versions.",
  EDITOR: "Editor: edits documents and boards and saves and restores versions.",
  ADMIN: "Admin: everything an editor can do, plus members and the audit trail.",
};

const ROLE_OPTIONS = ROLES.map((role) => ({ value: role, label: role }));

function joined(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** Frames 34 to 37: the members and their roles. Only admins can change anything. */
export function MembersPage() {
  const workspace = useWorkspace();
  const { session } = useSession();
  const myId = session.status === "authed" ? session.user.id : null;
  const isAdmin = workspace.role === "ADMIN";
  const { toast } = useToast();
  const [state, setState] = useState<State>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [inviting, setInviting] = useState(false);
  const [changing, setChanging] = useState<{ member: Member; to: Role } | null>(null);
  const [removing, setRemoving] = useState<Member | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.workspaces
      .members(workspace.id)
      .then((members) => !cancelled && setState({ status: "ready", members }))
      .catch(() => !cancelled && setState({ status: "error" }));
    return () => {
      cancelled = true;
    };
  }, [workspace.id, attempt]);

  function put(member: Member) {
    setState((current) =>
      current.status === "ready"
        ? {
            status: "ready",
            members: current.members.some((m) => m.user_id === member.user_id)
              ? current.members.map((m) => (m.user_id === member.user_id ? member : m))
              : [...current.members, member],
          }
        : current,
    );
  }

  return (
    <div className="mx-auto max-w-4xl">
      <div className="mb-2 flex items-center justify-between gap-4">
        <h1 className="text-heading-lg">Members &amp; access</h1>
        {isAdmin && state.status === "ready" && (
          <Button onClick={() => setInviting(true)}>+ Invite Member</Button>
        )}
      </div>
      <p className="mb-6 text-ui-sm text-text-muted">
        Only admins can manage members. Owner access is locked.
      </p>

      {state.status === "loading" && (
        <div aria-busy="true" aria-label="Loading members" className="flex flex-col gap-2">
          {[0, 1, 2].map((n) => (
            <div key={n} className="h-12 animate-pulse rounded-card bg-surface" />
          ))}
        </div>
      )}

      {state.status === "error" && (
        <Banner
          tone="danger"
          title="Could not load the members."
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

      {state.status === "ready" && (
        <table className="w-full border-collapse text-left text-ui-base">
          <thead>
            <tr className="border-b border-border text-micro text-text-muted uppercase">
              <th className="py-2 pr-4 font-semibold">Member</th>
              <th className="py-2 pr-4 font-semibold">Email</th>
              <th className="py-2 pr-4 font-semibold">Role</th>
              <th className="py-2 pr-4 font-semibold">Joined</th>
              <th className="py-2 font-semibold">
                <span className="sr-only">Action</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {state.members.map((member) => {
              const self = member.user_id === myId;
              return (
                <tr key={member.user_id} className="border-b border-border">
                  <td className="py-3 pr-4">
                    <span className="flex items-center gap-2">
                      <Avatar name={member.name} size="sm" />
                      <span>{member.name}</span>
                      {self && <span className="text-micro text-text-muted">(you)</span>}
                    </span>
                  </td>
                  <td className="py-3 pr-4 text-text-muted">{member.email}</td>
                  <td className="py-3 pr-4">
                    {member.is_owner ? (
                      <span className="flex items-center gap-1">
                        <span aria-hidden="true">🔒</span>
                        <span>ADMIN (Owner)</span>
                      </span>
                    ) : isAdmin ? (
                      <div className="w-32">
                        <Select
                          label={`Role of ${member.name}`}
                          value={member.role}
                          options={ROLE_OPTIONS}
                          onChange={(e) =>
                            e.target.value !== member.role &&
                            setChanging({ member, to: e.target.value as Role })
                          }
                        />
                      </div>
                    ) : (
                      member.role
                    )}
                  </td>
                  <td className="py-3 pr-4 text-text-muted">{joined(member.joined_at)}</td>
                  <td className="py-3 text-right">
                    {isAdmin && !member.is_owner && !self && (
                      <Button
                        variant="ghost"
                        aria-label={`Remove ${member.name}`}
                        onClick={() => setRemoving(member)}
                      >
                        Remove
                      </Button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}

      {inviting && (
        <InviteDialog
          workspaceId={workspace.id}
          onClose={() => setInviting(false)}
          onAdded={(member) => {
            put(member);
            setInviting(false);
            toast({ title: `${member.name} was added as ${member.role}`, tone: "success" });
          }}
        />
      )}

      {changing && (
        <Dialog
          open
          onClose={() => setChanging(null)}
          title={`Change role of ${changing.member.name}?`}
          description={`${changing.member.role} becomes ${changing.to}.`}
          footer={
            <>
              <Button variant="secondary" onClick={() => setChanging(null)}>
                Cancel
              </Button>
              <ConfirmRole
                workspaceId={workspace.id}
                change={changing}
                onDone={(member) => {
                  put(member);
                  setChanging(null);
                  toast({
                    title: `${member.name} is now ${member.role}`,
                    tone: "success",
                  });
                }}
                onFailed={() => {
                  setChanging(null);
                  toast({ title: "Could not change the role", tone: "danger" });
                  setAttempt((n) => n + 1);
                }}
              />
            </>
          }
        >
          <ul className="flex flex-col gap-1 text-ui-sm text-text-muted">
            {ROLES.map((role) => (
              <li key={role} className={role === changing.to ? "text-text" : undefined}>
                {ROLE_TEXT[role]}
              </li>
            ))}
          </ul>
          <div className="mt-4">
            <Banner tone="warning">Role changes apply to open sessions immediately.</Banner>
          </div>
        </Dialog>
      )}

      {removing && (
        <Dialog
          open
          onClose={() => setRemoving(null)}
          title={`Remove ${removing.name}?`}
          description="They lose access to this workspace and are taken off its tasks. This is recorded in the audit trail."
          footer={
            <>
              <Button variant="secondary" onClick={() => setRemoving(null)}>
                Cancel
              </Button>
              <ConfirmRemove
                workspaceId={workspace.id}
                member={removing}
                onDone={() => {
                  setState((current) =>
                    current.status === "ready"
                      ? {
                          status: "ready",
                          members: current.members.filter((m) => m.user_id !== removing.user_id),
                        }
                      : current,
                  );
                  setRemoving(null);
                  toast({ title: `${removing.name} was removed`, tone: "success" });
                }}
                onFailed={() => {
                  setRemoving(null);
                  toast({ title: "Could not remove the member", tone: "danger" });
                  setAttempt((n) => n + 1);
                }}
              />
            </>
          }
        />
      )}
    </div>
  );
}

function ConfirmRole({
  workspaceId,
  change,
  onDone,
  onFailed,
}: {
  workspaceId: string;
  change: { member: Member; to: Role };
  onDone(member: Member): void;
  onFailed(): void;
}) {
  const [busy, setBusy] = useState(false);
  return (
    <Button
      loading={busy}
      onClick={async () => {
        setBusy(true);
        try {
          onDone(
            await api.workspaces.changeRole(workspaceId, change.member.user_id, {
              role: change.to,
            }),
          );
        } catch {
          onFailed();
        } finally {
          setBusy(false);
        }
      }}
    >
      Change role
    </Button>
  );
}

function ConfirmRemove({
  workspaceId,
  member,
  onDone,
  onFailed,
}: {
  workspaceId: string;
  member: Member;
  onDone(): void;
  onFailed(): void;
}) {
  const [busy, setBusy] = useState(false);
  return (
    <Button
      variant="danger"
      loading={busy}
      onClick={async () => {
        setBusy(true);
        try {
          await api.workspaces.removeMember(workspaceId, member.user_id);
          onDone();
        } catch {
          onFailed();
        } finally {
          setBusy(false);
        }
      }}
    >
      Remove member
    </Button>
  );
}

/** Frame 35: the email must belong to a registered account. */
function InviteDialog({
  workspaceId,
  onClose,
  onAdded,
}: {
  workspaceId: string;
  onClose(): void;
  onAdded(member: Member): void;
}) {
  const [email, setEmail] = useState("");
  const [role, setRole] = useState<Role>("EDITOR");
  const [emailError, setEmailError] = useState<string | undefined>();
  const [formError, setFormError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const valid = email.trim() !== "";

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setEmailError(undefined);
    setFormError(null);
    try {
      onAdded(await api.workspaces.addMember(workspaceId, { email: email.trim(), role }));
    } catch (e) {
      if (isApiError(e) && e.status === 404) setEmailError("No SyncSpace account uses this email");
      else if (isApiError(e) && e.status === 409) setEmailError("This person is already a member");
      else if (isApiError(e) && e.status === 422) setEmailError(e.details.email ?? e.details.role);
      else if (isApiError(e) && e.status === 403)
        setFormError("Only admins can add members to this workspace.");
      else setFormError("Could not add the member. Check your connection and try again.");
    } finally {
      setLoading(false);
    }
  }

  return (
    <Dialog
      open
      onClose={onClose}
      title="Add workspace member"
      description="The person needs a SyncSpace account."
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        <Input
          label="Email"
          type="email"
          value={email}
          error={emailError}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Select
          label="Role"
          value={role}
          options={ROLE_OPTIONS}
          onChange={(e) => setRole(e.target.value as Role)}
        />
        {formError && <Banner tone="danger">{formError}</Banner>}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!valid} loading={loading}>
            Add member
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
