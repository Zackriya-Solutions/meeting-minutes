/** @type {import('tailwindcss').Config} */

// Every color reads its value from a custom property defined in
// src/app/globals.css. Values are stored as bare OKLCH components so the
// `<alpha-value>` placeholder below gives us working opacity modifiers
// (`bg-brand/40`). globals.css is the source of truth — see /DESIGN.md.
const c = (v) => `oklch(var(${v}) / <alpha-value>)`

module.exports = {
  darkMode: ['class'],
  content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['var(--font-sans)', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        serif: ['var(--font-serif)', 'ui-serif', 'Georgia', 'serif'],
        mono: ['var(--font-mono)', 'ui-monospace', 'SFMono-Regular', 'monospace'],
      },

      // Fixed rem scale. No clamp() — this is a desktop app viewed at a
      // consistent DPI, and fluid type in a 256px rail looks worse.
      fontSize: {
        '2xs': ['0.6875rem', { lineHeight: '0.9375rem' }], // 11/15 · mono readouts
        xs: ['0.75rem', { lineHeight: '1.0625rem' }],      // 12/17 · captions
        sm: ['0.8125rem', { lineHeight: '1.1875rem' }],    // 13/19 · dense labels
        base: ['0.875rem', { lineHeight: '1.3125rem' }],   // 14/21 · UI body
        md: ['0.9375rem', { lineHeight: '1.5rem' }],       // 15/24 · transcript
        lg: ['1.0625rem', { lineHeight: '1.6875rem' }],    // 17/27 · summary
        xl: ['1.25rem', { lineHeight: '1.6875rem' }],      // 20/27 · panel title
        '2xl': ['1.5625rem', { lineHeight: '1.9375rem' }], // 25/31 · page title
        '3xl': ['1.9375rem', { lineHeight: '2.3125rem' }], // 31/37 · meeting title
      },

      colors: {
        // --- semantic system (use these) ---------------------------------
        canvas: c('--bg'),
        panel: c('--panel'),
        elevated: c('--elevated'),
        sunken: c('--sunken'),
        line: {
          DEFAULT: c('--border-c'),
          strong: c('--border-strong'),
        },
        ink: {
          DEFAULT: c('--ink'),
          muted: c('--ink-muted'),
          faint: c('--ink-faint'),
        },
        brand: {
          DEFAULT: c('--brand'),
          hover: c('--brand-hover'),
          ink: c('--brand-ink'),
          soft: c('--brand-soft'),
          'soft-ink': c('--brand-soft-ink'),
        },
        danger: {
          DEFAULT: c('--danger'),
          hover: c('--danger-hover'),
          ink: c('--danger-ink'),
          soft: c('--danger-soft'),
        },
        warn: {
          DEFAULT: c('--warn'),
          ink: c('--warn-ink'),
          soft: c('--warn-soft'),
        },
        info: {
          DEFAULT: c('--info'),
          ink: c('--info-ink'),
          soft: c('--info-soft'),
        },

        // --- shadcn compat -------------------------------------------------
        // components/ui/* reference these. Mapped in globals.css onto the
        // tokens above so the primitive layer inherits the system for free.
        // Do not reach for these names in new code.
        background: c('--background'),
        foreground: c('--foreground'),
        border: c('--border'),
        input: c('--input'),
        ring: c('--ring'),
        primary: {
          DEFAULT: c('--primary'),
          foreground: c('--primary-foreground'),
        },
        secondary: {
          DEFAULT: c('--secondary'),
          foreground: c('--secondary-foreground'),
        },
        card: {
          DEFAULT: c('--card'),
          foreground: c('--card-foreground'),
        },
        popover: {
          DEFAULT: c('--popover'),
          foreground: c('--popover-foreground'),
        },
        muted: {
          DEFAULT: c('--muted'),
          foreground: c('--muted-foreground'),
        },
        accent: {
          DEFAULT: c('--accent'),
          foreground: c('--accent-foreground'),
        },
        destructive: {
          DEFAULT: c('--destructive'),
          foreground: c('--destructive-foreground'),
        },
      },

      borderRadius: {
        sm: 'var(--r-sm)',
        md: 'var(--r-md)',
        lg: 'var(--r-lg)',
        xl: 'var(--r-xl)',
      },

      boxShadow: {
        pop: 'var(--shadow-pop)',
        float: 'var(--shadow-float)',
      },

      // Semantic scale only. No arbitrary 999s.
      zIndex: {
        dropdown: 'var(--z-dropdown)',
        sticky: 'var(--z-sticky)',
        rail: 'var(--z-rail)',
        overlay: 'var(--z-overlay)',
        modal: 'var(--z-modal)',
        toast: 'var(--z-toast)',
        tooltip: 'var(--z-tooltip)',
      },

      transitionTimingFunction: {
        DEFAULT: 'var(--ease)',
        ease: 'var(--ease)',
      },

      transitionDuration: {
        DEFAULT: 'var(--dur)',
        fast: 'var(--dur-fast)',
        slow: 'var(--dur-slow)',
      },

      maxWidth: {
        measure: 'var(--measure)',
      },

      spacing: {
        rail: 'var(--rail)',
      },

      keyframes: {
        'accordion-down': {
          from: { height: '0' },
          to: { height: 'var(--radix-accordion-content-height)' },
        },
        'accordion-up': {
          from: { height: 'var(--radix-accordion-content-height)' },
          to: { height: '0' },
        },
      },

      animation: {
        'accordion-down': 'accordion-down var(--dur) var(--ease)',
        'accordion-up': 'accordion-up var(--dur) var(--ease)',
      },
    },
  },
  plugins: [require('tailwindcss-animate'), require('@tailwindcss/typography')],
}
