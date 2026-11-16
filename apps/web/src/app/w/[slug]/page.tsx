import { SlugRedirect } from "./SlugRedirect";

/** `/w/[slug]` goes to `/w/[slug]/docs`. */
export default function WorkspaceIndexPage() {
  return <SlugRedirect to="docs" />;
}
