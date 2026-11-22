"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { api, isApiError } from "@/lib/api";
import { useSession } from "@/lib/session";
import { AuthCard } from "./AuthCard";
import { GithubButton } from "./GithubButton";

/**
 * The free API host sleeps when idle and takes up to a minute to wake: the first request then
 * ends in a network error or a 502, 503 or 504 from the proxy, and that request is what wakes the
 * server. Signing in is safe to repeat, so the form tries again by itself and says why.
 */
export const WAKE_ATTEMPTS = 8;
export const WAKE_WAIT_MS = 6000;

/** What each `?github_error=` code from the GitHub callback means for the person. */
export const GITHUB_ERRORS: Record<string, string> = {
  denied: "GitHub sign-in was cancelled.",
  state: "The GitHub sign-in expired or was not started on this page. Please try again.",
  failed: "Could not sign in with GitHub. Try again in a moment.",
  no_email:
    "Your GitHub account has no verified email address. Verify one on GitHub, or create an account with your email.",
  conflict: "This GitHub account, or the SyncSpace account, is already linked to another one.",
};

function serverNotAnswering(error: unknown): boolean {
  return (
    isApiError(error) &&
    (error.code === "NETWORK" ||
      error.status === 502 ||
      error.status === 503 ||
      error.status === 504)
  );
}

export function LoginForm() {
  const router = useRouter();
  const githubError = useSearchParams().get("github_error");
  const { signedIn } = useSession();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [waking, setWaking] = useState(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const valid = email.trim() !== "" && password !== "";

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setError(null);
    for (let attempt = 1; ; attempt += 1) {
      try {
        const user = await api.auth.login({ email: email.trim(), password });
        signedIn(user);
        router.replace("/w");
        return;
      } catch (e) {
        if (!mounted.current) return;
        if (serverNotAnswering(e) && attempt < WAKE_ATTEMPTS) {
          setWaking(true);
          await new Promise((resolve) => setTimeout(resolve, WAKE_WAIT_MS));
          if (!mounted.current) return;
          continue;
        }
        setWaking(false);
        setError(
          isApiError(e) && e.status === 401
            ? "Invalid email or password."
            : isApiError(e) && e.status === 429
              ? "Too many attempts. Try again in a moment."
              : serverNotAnswering(e)
                ? "Could not sign in. The server did not answer; it may still be waking up. Wait a minute and try again."
                : "Could not sign in. Check your connection and try again.",
        );
        setLoading(false);
        return;
      }
    }
  }

  return (
    <AuthCard
      title="Sign in"
      subtitle="Welcome back to SyncSpace."
      footer={
        <>
          New here?{" "}
          <Link href="/register" className="text-indigo hover:underline">
            Create an account
          </Link>
        </>
      }
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        {githubError !== null && (
          <Banner tone="danger" title="GitHub sign-in failed">
            {GITHUB_ERRORS[githubError] ?? GITHUB_ERRORS.failed}
          </Banner>
        )}
        <GithubButton />
        <Input
          label="Email"
          type="email"
          autoComplete="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Input
          label="Password"
          type="password"
          autoComplete="current-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        {waking && !error && (
          <Banner tone="info" title="Waking up the server">
            The free hosting sleeps when idle. This can take up to a minute; sign-in continues by
            itself.
          </Banner>
        )}
        {error && (
          <Banner tone="danger" title="Sign-in failed">
            {error}
          </Banner>
        )}
        <Button type="submit" disabled={!valid} loading={loading}>
          Sign in
        </Button>
      </form>
    </AuthCard>
  );
}
