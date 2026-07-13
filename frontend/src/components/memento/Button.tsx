import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { cn } from '@/lib/utils';

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> { variant?: 'primary' | 'secondary' | 'ghost'; size?: 'sm' | 'md'; icon?: ReactNode; }

export function Button({ variant = 'primary', size = 'md', icon, className, children, ...props }: ButtonProps) {
  return <button className={cn('mm-button mm-press', `mm-button-${variant}`, size === 'sm' && 'mm-button-sm', className)} {...props}>{icon}{children}</button>;
}
