import React from 'react';

// VALUEOS: Value Accelerator brand mark — blue tile with white "V ✦ A".
// A compact, self-contained rendering of the VA identity (brand blue #0030BC). The
// full-fidelity source logo lives at valueos/branding/source/ and
// frontend/src/valueos/assets/valueos-agent-logo.svg for reference.
export const VA_BLUE = '#0030BC';

export function VaLogo({ size = 96, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 120 120"
      role="img"
      aria-label="Value Accelerator"
      className={className}
      xmlns="http://www.w3.org/2000/svg"
    >
      <rect width="120" height="120" rx="24" fill={VA_BLUE} />
      <text
        x="31"
        y="80"
        textAnchor="middle"
        fontFamily="Arial, Helvetica, sans-serif"
        fontWeight="800"
        fontSize="54"
        fill="#ffffff"
      >
        V
      </text>
      {/* 4-point star between the letters */}
      <path d="M60 44 l6 14 14 6 -14 6 -6 14 -6 -14 -14 -6 14 -6 z" fill="#ffffff" />
      <text
        x="89"
        y="80"
        textAnchor="middle"
        fontFamily="Arial, Helvetica, sans-serif"
        fontWeight="800"
        fontSize="54"
        fill="#ffffff"
      >
        A
      </text>
    </svg>
  );
}
