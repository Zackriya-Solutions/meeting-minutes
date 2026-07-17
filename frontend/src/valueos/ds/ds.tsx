'use client';
// VALUEOS: the Value Accelerator design system, ported from the design handoff (_ds/*).
// One <style> block (injected once by <DesignSystem/> in the app root) carries the brand
// tokens, keyframes, and component classes; screens use semantic classNames. CSP allows
// inline <style> (see valueos/branding). Fonts fall back to system-ui when Manrope isn't
// bundled (bundling Manrope woff2 is a pixel-fidelity follow-up).
import React from 'react';

export const DS_CSS = `
:root {
  --va-blue:#1A1AFF; --va-blue-text-dark:#4040FF; --va-blue-dark:#0F0FCC;
  --va-blue-glow:rgba(26,26,255,.30); --va-blue-glow-strong:rgba(26,26,255,.50);
  --va-dark-bg:#0B0B1E; --va-dark-card:#131332; --va-dark-card-2:#18183D;
  --va-dark-border:#1E1E3D; --va-dark-border-soft:rgba(255,255,255,.08);
  --va-white:#FFFFFF; --va-gray-100:#F5F5FA; --va-gray-200:#E8E8F0;
  --va-gray-400:#AAAABB; --va-gray-600:#666680; --va-gray-800:#33333F; --va-near-black:#0D0D14;
  --va-muted-purple:#7777AA; --va-secondary:#9999BB;
  --va-signal-green:#2CB786; --va-signal-red:#CE3644;
  --font-display:'Manrope','DM Sans',system-ui,-apple-system,sans-serif;
  --font-ui:'Manrope',system-ui,-apple-system,sans-serif;
  --font-doc:'Calibri','Carlito',Arial,sans-serif;
  --font-mono:ui-monospace,'Courier New',Menlo,monospace;
  --radius-sm:6px; --radius:10px; --radius-lg:16px; --radius-xl:24px; --radius-full:9999px;
  --shadow-card:0 1px 3px rgba(13,13,20,.06),0 8px 24px rgba(13,13,20,.06);
  --shadow-pop:0 24px 64px rgba(13,13,20,.18);
  --shadow-btn:0 0 24px rgba(26,26,255,.35); --shadow-btn-hover:0 0 40px rgba(26,26,255,.55);
  --ease:cubic-bezier(0.16,1,0.3,1);
}
@keyframes vaPulse{0%,100%{opacity:1;transform:scale(1)}50%{opacity:.35;transform:scale(.85)}}
@keyframes vaWave{0%,100%{transform:scaleY(.35)}50%{transform:scaleY(1)}}
@keyframes vaSpin{to{transform:rotate(360deg)}}
@keyframes vaBlink{0%,49%{opacity:1}50%,100%{opacity:0}}
@keyframes vaFade{from{opacity:0;transform:translateY(6px)}to{opacity:1;transform:none}}

.va-root, .va-root *{box-sizing:border-box}
.va-root{font-family:var(--font-ui);color:var(--va-near-black);height:100%}
.va-root button{font-family:inherit;cursor:pointer}
.va-scroll::-webkit-scrollbar{width:8px;height:8px}
.va-scroll::-webkit-scrollbar-thumb{background:rgba(120,120,150,.35);border-radius:8px}
.va-scroll-dark::-webkit-scrollbar-thumb{background:rgba(255,255,255,.14);border-radius:8px}

/* overline / labels */
.va-ovl{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;color:var(--va-muted-purple)}

/* buttons */
.va-btn{display:inline-flex;align-items:center;justify-content:center;gap:8px;border:0;border-radius:var(--radius-full);
  font-weight:700;font-size:15px;padding:12px 22px;transition:all var(--ease) 150ms;white-space:nowrap;text-decoration:none}
.va-btn-primary{background:var(--va-blue);color:#fff;box-shadow:var(--shadow-btn)}
.va-btn-primary:hover{background:var(--va-blue-dark);box-shadow:var(--shadow-btn-hover)}
.va-btn-primary:disabled{opacity:.5;box-shadow:none;cursor:default}
.va-btn-white{background:#fff;color:var(--va-blue)}
.va-btn-white:hover{background:var(--va-gray-100)}
.va-btn-ghost-light{background:transparent;color:var(--va-near-black);border:1px solid var(--va-gray-200)}
.va-btn-ghost-light:hover{border-color:var(--va-blue);color:var(--va-blue)}
.va-btn-outline-white{background:transparent;color:#fff;border:1px solid rgba(255,255,255,.4)}
.va-btn-outline-white:hover{border-color:#fff;background:rgba(255,255,255,.08)}
.va-btn-danger{background:var(--va-signal-red);color:#fff}
.va-btn-danger:hover{filter:brightness(.94)}
.va-btn-danger-outline{background:transparent;color:var(--va-signal-red);border:1px solid var(--va-signal-red)}
.va-btn-danger-outline:hover{background:rgba(206,54,68,.08)}
.va-btn-sm{padding:8px 14px;font-size:13.5px}

/* status pills */
.va-pill{display:inline-flex;align-items:center;gap:7px;font-size:12px;font-weight:700;padding:3px 11px;border-radius:var(--radius-full);letter-spacing:.01em}
.va-pill-onair{background:rgba(206,54,68,.12);color:var(--va-signal-red)}
.va-pill-pending{background:var(--va-gray-100);color:var(--va-gray-600)}
.va-pill-syncing{background:rgba(26,26,255,.10);color:var(--va-blue)}
.va-pill-synced{background:rgba(44,183,134,.12);color:var(--va-signal-green)}
.va-pill-failed{background:rgba(206,54,68,.12);color:var(--va-signal-red)}
.va-dot{width:8px;height:8px;border-radius:50%;display:inline-block}
.va-dot-red{background:var(--va-signal-red);animation:vaPulse 1.4s var(--ease) infinite}

/* onboarding (full-bleed electric blue) */
.va-onb{position:absolute;inset:0;background:var(--va-blue);color:#fff;display:flex;flex-direction:column;
  align-items:center;justify-content:center;text-align:center;padding:24px;animation:vaFade .3s var(--ease);font-family:var(--font-ui)}
.va-onb h1{font-family:var(--font-display);font-weight:800;letter-spacing:-.02em;font-size:34px;margin:20px 0 10px;text-wrap:balance}
.va-onb p{font-family:var(--font-doc);font-size:16px;line-height:1.6;opacity:.92;max-width:460px;margin:0 0 30px}
.va-onb .va-foot{position:absolute;left:0;right:0;bottom:20px;font-size:12px;letter-spacing:.06em;opacity:.62}
.va-onb .va-path{font-family:var(--font-mono);font-size:13px;background:rgba(255,255,255,.12);padding:9px 13px;border-radius:8px;margin:16px 0}
.va-onb .va-err{color:#FFD7D7;font-size:14px;margin:14px 0 0;max-width:420px;line-height:1.5}
.va-spinner{width:44px;height:44px;border-radius:50%;border:4px solid rgba(255,255,255,.25);border-top-color:#fff;animation:vaSpin 1s linear infinite}
.va-track{width:280px;max-width:70vw;height:6px;border-radius:999px;background:rgba(255,255,255,.18);overflow:hidden;margin:18px 0 8px}
.va-track > i{display:block;height:100%;background:#fff;border-radius:999px;transition:width .3s linear}

/* dark-sidebar shell */
.va-shell{position:absolute;inset:0;display:flex;background:var(--va-white)}
.va-sidebar{width:236px;flex:0 0 auto;background:var(--va-dark-bg);color:#fff;display:flex;flex-direction:column;
  padding:20px 14px;border-right:1px solid var(--va-dark-border)}
.va-brand{display:flex;align-items:center;gap:10px;padding:6px 8px 4px}
.va-brand .bn{font-family:var(--font-display);font-weight:800;font-size:16px;letter-spacing:-.01em;line-height:1}
.va-brand .bs{font-size:11px;color:var(--va-muted-purple);margin-top:2px}
.va-navlist{display:flex;flex-direction:column;gap:2px;margin-top:22px}
.va-navitem{display:flex;align-items:center;gap:11px;width:100%;text-align:left;background:transparent;border:0;color:var(--va-secondary);
  font-size:14px;font-weight:600;padding:10px 12px;border-radius:8px;transition:all var(--ease) 150ms}
.va-navitem:hover{background:var(--va-dark-card);color:#fff}
.va-navitem.on{background:var(--va-dark-card-2);color:#fff}
.va-navitem.on .va-ic{color:var(--va-blue-text-dark)}
.va-ic{width:18px;height:18px;flex:0 0 auto;stroke:currentColor;stroke-width:2;fill:none;stroke-linecap:round;stroke-linejoin:round}
.va-navspacer{flex:1}
.va-content{flex:1;min-width:0;overflow-y:auto;background:var(--va-white)}
.va-page{max-width:1000px;margin:0 auto;padding:28px 32px 56px}
.va-page-head{display:flex;align-items:flex-start;justify-content:space-between;gap:16px;margin-bottom:24px}
.va-page-head h1{font-family:var(--font-display);font-weight:800;letter-spacing:-.02em;font-size:32px;margin:4px 0 0}

/* cards */
.va-card{background:#fff;border:1px solid var(--va-gray-200);border-radius:var(--radius);box-shadow:var(--shadow-card);
  transition:all var(--ease) 150ms}
.va-card-hover:hover{transform:translateY(-2px);border-color:var(--va-blue)}
.va-stat{font-family:var(--font-display);font-weight:800;font-size:44px;letter-spacing:-.02em;color:var(--va-blue);line-height:1}
.va-delta{color:var(--va-signal-green);font-size:13px;font-weight:700;margin-top:8px}
.va-muted{color:var(--va-gray-600)}
.va-body{font-family:var(--font-doc);color:var(--va-gray-800);line-height:1.6}

/* inputs */
.va-input{width:100%;padding:11px 13px;border-radius:8px;border:1px solid var(--va-gray-200);background:#fff;
  color:var(--va-near-black);font:inherit;font-size:14px}
.va-input:focus{outline:none;border-color:var(--va-blue);box-shadow:0 0 0 3px var(--va-blue-glow)}
.va-input-dark{background:rgba(255,255,255,.06);border:1px solid var(--va-dark-border);color:#fff}

/* modal / wizard */
.va-scrim{position:absolute;inset:0;background:rgba(11,11,30,.5);backdrop-filter:blur(4px);display:flex;
  align-items:center;justify-content:center;padding:24px;z-index:50;animation:vaFade .2s var(--ease)}
.va-modal{width:100%;max-width:520px;background:#fff;border-radius:var(--radius-lg);box-shadow:var(--shadow-pop);
  display:flex;flex-direction:column;max-height:calc(100% - 32px);overflow:hidden}
.va-modal-head{display:flex;align-items:center;justify-content:space-between;padding:18px 20px 0}
.va-seg{display:flex;gap:6px;padding:14px 20px}
.va-seg > i{flex:1;height:4px;border-radius:999px;background:var(--va-gray-200)}
.va-seg > i.on{background:var(--va-blue)}
.va-modal-body{padding:4px 20px 8px;overflow-y:auto}
.va-modal-foot{display:flex;justify-content:space-between;gap:10px;padding:16px 20px;border-top:1px solid var(--va-gray-200)}
.va-choice{display:block;width:100%;text-align:left;background:#fff;border:1px solid var(--va-gray-200);border-radius:10px;
  padding:14px;margin-bottom:8px;transition:all var(--ease) 150ms}
.va-choice:hover{border-color:var(--va-blue)}
.va-choice.on{border-color:var(--va-blue);box-shadow:0 0 0 3px var(--va-blue-glow)}
`;

/** Injects the design system once. Put near the app root. */
export function DesignSystem() {
  return <style dangerouslySetInnerHTML={{ __html: DS_CSS }} />;
}

/** V✦A mark. Blue on light surfaces, white on dark — never the reverse. */
export function VaMark({ height = 26, tone = 'blue' }: { height?: number; tone?: 'blue' | 'white' }) {
  const fill = tone === 'white' ? '#FFFFFF' : '#1A1AFF';
  return (
    <svg height={height} viewBox="0 0 179.58 100" role="img" aria-label="Value Accelerator" xmlns="http://www.w3.org/2000/svg">
      <path fill={fill} d="M62.42,25.81h14.43l-16.78,48.39h-17.33L25.89,25.81h14.5l11.05,34.93,10.98-34.93Z" />
      <path fill={fill} d="M136.06,25.81h-15.63l-.03.08-8.41,22.98h0s-1.44,3.95-1.44,3.95l-7.82,21.38h14.25l2.57-7.72h17.29l2.55,7.72h14.31l-17.63-48.39ZM122.98,56.14l5.22-15.7,5.2,15.7h-10.42Z" />
      <path fill={fill} d="M102.65,50.02c-.37.03-.68.08-.99.14-6.02.98-10.77,5.73-11.75,11.75-.06.3-.11.61-.14.98-.03-.36-.08-.66-.14-.96-.98-6.03-5.74-10.79-11.77-11.77-.31-.06-.62-.11-.99-.14.37-.03.68-.08.99-.14,6.03-.98,10.79-5.74,11.77-11.77.06-.3.1-.61.14-.97.03.37.08.67.14.98.98,6.02,5.73,10.77,11.75,11.75.31.06.62.11.99.14Z" />
    </svg>
  );
}
