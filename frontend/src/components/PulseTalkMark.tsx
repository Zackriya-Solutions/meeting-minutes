import type { SVGProps } from 'react'

export function PulseTalkMark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 64 64" fill="none" aria-hidden="true" {...props}>
      <path
        d="M12 14c0-5 4-9 9-9h22c5 0 9 4 9 9v23c0 5-4 9-9 9H31L18 57V46c-4-1-6-4-6-9V14Z"
        fill="currentColor"
      />
      <path
        d="M20 28h6l4-10 6 20 4-10h5"
        stroke="white"
        strokeWidth="5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}
