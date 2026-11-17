import { SlugRedirect } from "../SlugRedirect";

/** The general settings tab is not built; `/settings` goes to the members tab. */
export default function SettingsIndexPage() {
  return <SlugRedirect to="settings/members" />;
}
