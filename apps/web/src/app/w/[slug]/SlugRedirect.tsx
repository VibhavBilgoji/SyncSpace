"use client";

import { useParams, useRouter } from "next/navigation";
import { useEffect } from "react";

export function SlugRedirect({ to }: { to: string }) {
  const { slug } = useParams<{ slug: string }>();
  const router = useRouter();
  useEffect(() => {
    router.replace(`/w/${slug}/${to}`);
  }, [router, slug, to]);
  return null;
}
