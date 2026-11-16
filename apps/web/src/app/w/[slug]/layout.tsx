import { Suspense, type ReactNode } from "react";

/**
 * Pages under `/w/[slug]` read the slug with `useParams()`, which needs a Suspense boundary
 * when Cache Components is on. The shell shows its own skeleton once it mounts.
 */
export default function WorkspaceLayout({ children }: { children: ReactNode }) {
  return <Suspense fallback={null}>{children}</Suspense>;
}
