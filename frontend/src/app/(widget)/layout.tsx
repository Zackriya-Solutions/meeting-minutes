// Isolated root layout for the floating recording widget window (issue #718).
//
// This is a Next.js "multiple root layouts" route group: it defines its own
// <html>/<body>, completely separate from `app/(main)/layout.tsx` (which wraps
// the main window's routes with the sidebar, onboarding gate, and app-wide
// context providers). Route groups don't affect the URL, so this still
// statically exports to `/widget` under `output: 'export'` -- it just isn't
// nested under the main window's provider tree, which would otherwise render
// the entire app UI squeezed into a 260x70 undecorated window.
//
// Reuses globals.css for Tailwind + design tokens, but overrides the body
// background inline (higher specificity than the `bg-background` utility
// class) so the widget's undecorated, `transparent: true` window shows
// through instead of painting an opaque background behind the pill UI.
import '../globals.css';

export default function WidgetLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body
        className="font-sans antialiased"
        style={{ backgroundColor: 'transparent', margin: 0, height: '100vh', overflow: 'hidden' }}
      >
        {children}
      </body>
    </html>
  );
}
