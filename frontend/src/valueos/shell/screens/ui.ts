// VALUEOS: shared inline styles for the flow screens — solid Value Accelerator blue,
// white text (no white page backgrounds).
import type { CSSProperties } from 'react';
import { VA_BLUE } from '../../assets/VaLogo';

export const page: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'center',
  background: VA_BLUE,
  color: '#ffffff',
  fontFamily: 'system-ui, -apple-system, "Segoe UI", sans-serif',
  padding: 24,
  textAlign: 'center',
};

export const card: CSSProperties = {
  maxWidth: 520,
  width: '100%',
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
};

export const h1: CSSProperties = { fontSize: 30, fontWeight: 800, margin: '0 0 10px' };
export const sub: CSSProperties = { fontSize: 15, lineHeight: 1.5, opacity: 0.9, margin: '0 0 24px' };

export const primaryBtn: CSSProperties = {
  background: '#ffffff',
  color: VA_BLUE,
  border: 'none',
  borderRadius: 10,
  padding: '13px 30px',
  fontSize: 15,
  fontWeight: 700,
  cursor: 'pointer',
  marginTop: 8,
};
export const primaryBtnDisabled: CSSProperties = { ...primaryBtn, opacity: 0.45, cursor: 'not-allowed' };

export const ghostBtn: CSSProperties = {
  background: 'transparent',
  color: '#ffffff',
  border: '1px solid rgba(255,255,255,0.5)',
  borderRadius: 10,
  padding: '11px 24px',
  fontSize: 14,
  fontWeight: 600,
  cursor: 'pointer',
  marginTop: 8,
};

export const footer: CSSProperties = {
  position: 'absolute',
  bottom: 20,
  fontSize: 12,
  opacity: 0.7,
  letterSpacing: 1,
};
