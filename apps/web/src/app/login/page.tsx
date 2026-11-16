import { LoginForm } from "@/components/auth/LoginForm";
import { RedirectIfAuthed } from "@/lib/session";

export const metadata = { title: "Sign in · SyncSpace" };

export default function LoginPage() {
  return (
    <RedirectIfAuthed>
      <LoginForm />
    </RedirectIfAuthed>
  );
}
