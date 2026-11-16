"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";
import { Banner } from "@/components/ui/Banner";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { api, isApiError } from "@/lib/api";
import { isValidSlug, slugify } from "@/lib/slug";
import { RequireAuth } from "@/lib/session";
import { AuthCard } from "./AuthCard";

/** Frame 04. Also used to add another workspace later. */
export function OnboardingForm() {
  return (
    <RequireAuth>
      <OnboardingFormInner />
    </RequireAuth>
  );
}

function OnboardingFormInner() {
  const router = useRouter();
  const [name, setName] = useState("");
  const [customSlug, setCustomSlug] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [slugError, setSlugError] = useState<string | undefined>();
  const [nameError, setNameError] = useState<string | undefined>();
  const [formError, setFormError] = useState<string | null>(null);

  // The slug follows the name until the user edits it by hand.
  const slug = customSlug ?? slugify(name);

  const slugInvalid = slug !== "" && !isValidSlug(slug);
  const valid = name.trim() !== "" && isValidSlug(slug);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!valid || loading) return;
    setLoading(true);
    setSlugError(undefined);
    setNameError(undefined);
    setFormError(null);
    try {
      const workspace = await api.workspaces.create({ name: name.trim(), slug });
      router.replace(`/w/${workspace.slug}/docs`);
    } catch (e) {
      if (isApiError(e) && e.status === 409) {
        setSlugError("This slug is already taken.");
      } else if (isApiError(e) && e.status === 422) {
        setNameError(e.details.name);
        setSlugError(e.details.slug);
      } else {
        setFormError("Could not create the workspace. Check your connection and try again.");
      }
      setLoading(false);
    }
  }

  return (
    <AuthCard
      title="Create your first workspace"
      subtitle="Workspaces hold your documents and boards."
    >
      <form onSubmit={onSubmit} className="flex flex-col gap-4" noValidate>
        <Input
          label="Workspace name"
          maxLength={60}
          value={name}
          error={nameError}
          onChange={(e) => setName(e.target.value)}
        />
        <Input
          label="Slug"
          value={slug}
          hint="3 to 40 lowercase letters, digits or dashes"
          error={
            slugError ??
            (slugInvalid ? "Use 3 to 40 lowercase letters, digits or dashes." : undefined)
          }
          onChange={(e) => setCustomSlug(e.target.value)}
        />
        {formError && <Banner tone="danger">{formError}</Banner>}
        <Button type="submit" disabled={!valid} loading={loading}>
          Create workspace
        </Button>
      </form>
    </AuthCard>
  );
}
