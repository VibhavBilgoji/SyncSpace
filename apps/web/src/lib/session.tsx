"use client";

import { useRouter } from "next/navigation";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { api } from "@/lib/api";
import type { User } from "@/types/generated";

type SessionState = { status: "loading" } | { status: "anon" } | { status: "authed"; user: User };

interface SessionApi {
  session: SessionState;
  /** Call after a successful login or register. */
  signedIn: (user: User) => void;
  signOut: () => Promise<void>;
}

const SessionContext = createContext<SessionApi | null>(null);

/** Reads `GET /auth/me` once; the HttpOnly cookie is sent by the browser. */
export function SessionProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<SessionState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    api.auth
      .me()
      .then((user) => !cancelled && setSession({ status: "authed", user }))
      .catch(() => !cancelled && setSession({ status: "anon" }));
    return () => {
      cancelled = true;
    };
  }, []);

  const signedIn = useCallback((user: User) => setSession({ status: "authed", user }), []);
  const signOut = useCallback(async () => {
    try {
      await api.auth.logout();
    } finally {
      // Even if the call failed the UI must not keep showing a user the server may have dropped.
      setSession({ status: "anon" });
    }
  }, []);

  const value = useMemo(() => ({ session, signedIn, signOut }), [session, signedIn, signOut]);
  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionApi {
  const value = useContext(SessionContext);
  if (!value) throw new Error("useSession must be used inside <SessionProvider>");
  return value;
}

/** Renders children only for a signed-in user; sends everyone else to /login. */
export function RequireAuth({ children }: { children: ReactNode }) {
  const { session } = useSession();
  const router = useRouter();
  useEffect(() => {
    if (session.status === "anon") router.replace("/login");
  }, [session.status, router]);
  if (session.status !== "authed") return <FullPageSkeleton />;
  return <>{children}</>;
}

/** For /login and /register: a signed-in user goes on to the picker. */
export function RedirectIfAuthed({ children }: { children: ReactNode }) {
  const { session } = useSession();
  const router = useRouter();
  useEffect(() => {
    if (session.status === "authed") router.replace("/w");
  }, [session.status, router]);
  if (session.status === "authed") return <FullPageSkeleton />;
  return <>{children}</>;
}

function FullPageSkeleton() {
  return (
    <div
      aria-busy="true"
      aria-label="Loading"
      className="flex min-h-screen items-center justify-center"
    >
      <div className="h-6 w-40 animate-pulse rounded-control bg-surface" />
    </div>
  );
}
