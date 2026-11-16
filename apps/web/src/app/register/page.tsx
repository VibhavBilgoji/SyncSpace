import { RegisterForm } from "@/components/auth/RegisterForm";
import { RedirectIfAuthed } from "@/lib/session";

export const metadata = { title: "Create account · SyncSpace" };

export default function RegisterPage() {
  return (
    <RedirectIfAuthed>
      <RegisterForm />
    </RedirectIfAuthed>
  );
}
