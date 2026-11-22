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
import { GithubButton } from "./GithubButton";

type Field = "name" | "email" | "password";

export function RegisterForm() {
  const router = useRouter();
  const { signedIn } = useSession();
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<Partial<Record<Field, string>>>({});
  const [formError, setFormError] = useState<string | null>(null);

  const valid = name.trim() !== "" && email.trim() !== "" && [...password].length >= 8;

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setFieldErrors({});
    setFormError(null);
    try {
      const user = await api.auth.register({ name: name.trim(), email: email.trim(), password });
      signedIn(user);
      // The picker sends a user with no workspace on to onboarding.
      router.replace("/w");
    } catch (e) {
      if (isApiError(e) && e.status === 409) {
        setFieldErrors({ email: "An account with this email already exists." });
      } else if (isApiError(e) && e.status === 422) {
        setFieldErrors({
          name: e.details.name,
          email: e.details.email,
          password: e.details.password,
        });
      } else {
        setFormError("Could not create the account. Check your connection and try again.");
      }
      setLoading(false);
    }
  }

  return (
    <AuthCard
      title="Create account"
      subtitle="Start collaborating in real time."
      footer={
        <>
          Already have an account?{" "}
          <Link href="/login" className="text-indigo hover:underline">
            Sign in
          </Link>
        </>
      }
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        <GithubButton />
        <Input
          label="Name"
          autoComplete="name"
          maxLength={80}
          value={name}
          error={fieldErrors.name}
          onChange={(e) => setName(e.target.value)}
        />
        <Input
          label="Email"
          type="email"
          autoComplete="email"
          value={email}
          error={fieldErrors.email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Input
          label="Password"
          type="password"
          autoComplete="new-password"
          hint="8 to 128 characters"
          value={password}
          error={fieldErrors.password}
          onChange={(e) => setPassword(e.target.value)}
        />
        {formError && <Banner tone="danger">{formError}</Banner>}
        <Button type="submit" disabled={!valid} loading={loading}>
          Create account
        </Button>
      </form>
    </AuthCard>
  );
}
