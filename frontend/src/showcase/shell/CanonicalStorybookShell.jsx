import "./canonical-shell.css";

function ChevronIcon() {
  return (
    <svg className="storybook-chevron" viewBox="0 0 24 24" aria-hidden="true">
      <path d="m9 18 6-6-6-6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function BackIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="m15 18-6-6 6-6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function ThemeIcon({ theme }) {
  return theme === "dark" ? (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="12" cy="12" r="4" fill="none" stroke="currentColor" strokeWidth="2" />
      <path d="M12 2v2m0 16v2M4.93 4.93l1.42 1.42m11.3 11.3 1.42 1.42M2 12h2m16 0h2M4.93 19.07l1.42-1.42m11.3-11.3 1.42-1.42" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  ) : (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M20.5 14.2A8.5 8.5 0 0 1 9.8 3.5 8.5 8.5 0 1 0 20.5 14.2Z" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
    </svg>
  );
}

export default function CanonicalStorybookShell({
  groups,
  activeId,
  onSelect,
  onBack,
  theme = "light",
  onToggleTheme,
  children,
}) {
  const items = groups.flatMap((group) => group.items);
  const activeItem = items.find((item) => item.id === activeId);
  const nextTheme = theme === "dark" ? "light" : "dark";

  return (
    <div className="storybook-shell" data-theme={theme} data-selected={activeItem ? "true" : "false"}>
      <aside className="storybook-sidebar">
        <div className="storybook-sidebar-scroll">
          <header className="storybook-appbar">
            <span aria-hidden="true" />
            <div className="storybook-appbar-title">Storybook</div>
            <button className="storybook-appbar-action" type="button" onClick={onToggleTheme} aria-label={`Switch to ${nextTheme} theme`}>
              <ThemeIcon theme={theme} />
            </button>
          </header>

          <nav className="storybook-catalog" aria-label="Component catalog">
            {groups.map((group) => (
              <section key={group.id}>
                <h2 className="storybook-catalog-heading">{group.title}</h2>
                <div className="storybook-catalog-group">
                  {group.items.map((item) => (
                    <button
                      key={item.id}
                      className="storybook-catalog-cell"
                      type="button"
                      aria-current={activeItem?.id === item.id ? "page" : undefined}
                      onClick={() => onSelect(item.id)}
                    >
                      <span className="storybook-catalog-cell-label">{item.title}</span>
                      <ChevronIcon />
                    </button>
                  ))}
                </div>
              </section>
            ))}
          </nav>
        </div>
      </aside>

      <main className="storybook-detail">
        {activeItem ? (
          <>
            <header className="storybook-appbar">
              <button className="storybook-appbar-action storybook-back-action" type="button" onClick={onBack} aria-label="Back to catalog">
                <BackIcon />
              </button>
              <div className="storybook-appbar-title">{activeItem.title}</div>
              <span aria-hidden="true" />
            </header>
            <div className="storybook-preview">
              <div className="storybook-preview-content" data-storybook-preview="">
                {children}
              </div>
            </div>
          </>
        ) : (
          <div className="storybook-placeholder">Select a component</div>
        )}
      </main>
    </div>
  );
}
