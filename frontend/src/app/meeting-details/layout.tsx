import type { ReactNode } from "react";

import { MeetingDrawerShell } from "./meeting-drawer-shell";

export default function MeetingDetailsLayout({ children }: { children: ReactNode }) {
  return <MeetingDrawerShell>{children}</MeetingDrawerShell>;
}
