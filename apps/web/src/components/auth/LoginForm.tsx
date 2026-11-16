"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { api, isApiError } from "@/lib/api";
import { useSession } from "@/lib/session";
import { AuthCard } from "./AuthCard";

export function LoginForm() {
  const router = useRouter();
  const { signedIn } = useSession();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const valid = email.trim() !== "" && password !== "";

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setError(null);
    try {
      const user = await api.auth.login({ email: email.trim(), password });
      signedIn(user);
      router.replace("/w");
    } catch (e) {
      setError(
        isApiError(e) && e.status === 401
          ? "Invalid email or password."
          : isApiError(e) && e.status === 429
            ? "Too many attempts. Try again in a moment."
            : "Could not sign in. Check your connection and try again.",
      );
      setLoading(false);
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
