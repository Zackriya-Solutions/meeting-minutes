import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { resolveBuildInfo, BUILD_INFO } from '@/valueos/buildInfo';
import { BuildStamp } from '@/valueos/shell/BuildStamp';
import { LandingScreen } from '@/valueos/shell/screens/LandingScreen';

// VALUEOS: tests for OUR build-stamp feature (buildInfo resolver + BuildStamp component +
// landing wiring). Pure logic gets exhaustive coverage; the component/landing tests prove
// the stamp actually renders where the user will look.

describe('resolveBuildInfo', () => {
  it('CI build: exposes the short id + a formatted UTC time in the label', () => {
    const b = resolveBuildInfo({
      NEXT_PUBLIC_VALUEOS_BUILD: 'a1b2c3d',
      NEXT_PUBLIC_VALUEOS_BUILD_TIME: '2026-07-16T14:22:33Z',
    });
    expect(b.isLocal).toBe(false);
    expect(b.id).toBe('a1b2c3d');
    expect(b.time).toBe('2026-07-16T14:22:33Z');
    expect(b.label).toBe('build a1b2c3d · 2026-07-16 14:22 UTC');
  });

  it('a build id with no time omits the separator', () => {
    const b = resolveBuildInfo({ NEXT_PUBLIC_VALUEOS_BUILD: 'a1b2c3d' });
    expect(b.time).toBeNull();
    expect(b.label).toBe('build a1b2c3d');
  });

  it('no build id → "local build"', () => {
    const b = resolveBuildInfo({});
    expect(b.isLocal).toBe(true);
    expect(b.id).toBe('local');
    expect(b.label).toBe('local build');
  });

  it('a blank/whitespace build id is treated as local', () => {
    const b = resolveBuildInfo({ NEXT_PUBLIC_VALUEOS_BUILD: '   ', NEXT_PUBLIC_VALUEOS_BUILD_TIME: '  ' });
    expect(b.isLocal).toBe(true);
    expect(b.time).toBeNull();
    expect(b.label).toBe('local build');
  });

  it('a malformed time falls back to the raw value (never throws)', () => {
    const b = resolveBuildInfo({
      NEXT_PUBLIC_VALUEOS_BUILD: 'a1b2c3d',
      NEXT_PUBLIC_VALUEOS_BUILD_TIME: 'yesterday',
    });
    expect(b.label).toBe('build a1b2c3d · yesterday');
  });

  it('a non-UTC (offset) time is shown raw, never mislabeled as UTC', () => {
    const b = resolveBuildInfo({
      NEXT_PUBLIC_VALUEOS_BUILD: 'a1b2c3d',
      NEXT_PUBLIC_VALUEOS_BUILD_TIME: '2026-07-16T14:22:33+02:00',
    });
    expect(b.label).toBe('build a1b2c3d · 2026-07-16T14:22:33+02:00');
  });

  it('a time with no build id is dead data → "local build", time null', () => {
    const b = resolveBuildInfo({
      NEXT_PUBLIC_VALUEOS_BUILD: '',
      NEXT_PUBLIC_VALUEOS_BUILD_TIME: '2026-07-16T14:22:33Z',
    });
    expect(b.isLocal).toBe(true);
    expect(b.time).toBeNull();
    expect(b.label).toBe('local build');
  });
});

describe('BuildStamp component', () => {
  it('renders the exact injected build label (concrete, non-circular)', () => {
    const info = resolveBuildInfo({
      NEXT_PUBLIC_VALUEOS_BUILD: 'deadbee',
      NEXT_PUBLIC_VALUEOS_BUILD_TIME: '2026-01-02T03:04:05Z',
    });
    render(<BuildStamp info={info} />);
    expect(screen.getByTestId('valueos-build-stamp')).toHaveTextContent(
      'build deadbee · 2026-01-02 03:04 UTC',
    );
  });

  it('defaults to the ambient BUILD_INFO and is click-through / non-wrapping', () => {
    render(<BuildStamp />);
    const el = screen.getByTestId('valueos-build-stamp');
    expect(el).toHaveTextContent(BUILD_INFO.label);
    expect(el).toHaveStyle({ pointerEvents: 'none', position: 'fixed', whiteSpace: 'nowrap' });
  });
});

describe('LandingScreen wiring', () => {
  it('shows the build stamp in the corner (so a stale build is obvious)', () => {
    render(<LandingScreen onProceed={() => {}} />);
    expect(screen.getByTestId('valueos-build-stamp')).toBeInTheDocument();
  });
});

describe('buildInfo module inlines injected env at load time', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it('reads NEXT_PUBLIC_VALUEOS_BUILD / _TIME when present', async () => {
    vi.stubEnv('NEXT_PUBLIC_VALUEOS_BUILD', 'deadbee');
    vi.stubEnv('NEXT_PUBLIC_VALUEOS_BUILD_TIME', '2026-01-02T03:04:05Z');
    vi.resetModules();
    const mod = await import('@/valueos/buildInfo');
    expect(mod.BUILD_INFO.id).toBe('deadbee');
    expect(mod.BUILD_INFO.isLocal).toBe(false);
    expect(mod.BUILD_INFO.label).toBe('build deadbee · 2026-01-02 03:04 UTC');
  });
});
