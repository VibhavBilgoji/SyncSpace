"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { useWorkspace } from "@/components/shell/WorkspaceShell";
import { Banner } from "@/components/ui/Banner";
import { api } from "@/lib/api";
import { useDocumentSession } from "@/lib/doc/useDocumentSession";
import { DocumentEditor } from "./DocumentEditor";

type Meta =
  { status: "loading" } | { status: "missing" } | { status: "ready"; title: string; icon: string };

/** The route `/w/[slug]/docs/[docId]`: title and icon over REST, the blocks over the socket. */
export function DocumentPage() {
  const { docId } = useParams<{ docId: string }>();
  const workspace = useWorkspace();
  const session = useDocumentSession(docId);
  const [meta, setMeta] = useState<Meta>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    api.documents
      .get(docId)
      .then((d) => !cancelled && setMeta({ status: "ready", title: d.title, icon: d.icon }))
      .catch(() => !cancelled && setMeta({ status: "missing" }));
    return () => {
      cancelled = true;
    };
  }, [docId]);

  if (meta.status === "missing") {
    return <Banner tone="danger" title="Document not found." />;
  }
  return (
    <DocumentEditor
      title={meta.status === "ready" ? meta.title : ""}
      icon={meta.status === "ready" ? meta.icon : ""}
      role={workspace.role}
      session={session}
    />
  );
}
