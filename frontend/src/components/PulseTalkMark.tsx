import type { SVGProps } from 'react'

/**
 * Deep Focus brand mark: the lowercase "p" in Hot Signal on a Blackout tile.
 * Colour follows `currentColor` so callers can tint it with text utilities.
 */
export function PulseTalkMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 64 64" fill="none" aria-hidden="true" {...props}>
      <rect x="4" y="4" width="56" height="56" rx="8" fill="#0b0b0c" />
      <text
        x="32"
        y="47"
        textAnchor="middle"
        fontFamily="Archivo, 'Segoe UI', sans-serif"
        fontSize="44"
        fontWeight="500"
        letterSpacing="-2"
        fill="currentColor"
      >
        p
      </text>
    </svg>
  )
}
