'use client';

import React, {
  forwardRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
} from 'react';
import { Wordmark } from '@/components/memento/Wordmark';
import MotionProvider from '@/vendor/deslop/mini-app/components/MotionProvider';
import styles from './TypeOnboarding.module.css';

/**
 * These primitives are a TypeScript port of the components used by Type's onboarding:
 * AppshotsLogoHeader, StartView, SectionList, Cell and RegularButton.
 * Their composition and metrics stay aligned with /Documents/type/openwhispr.
 */

interface TypeOnboardingWindowProps extends HTMLAttributes<HTMLDivElement> {
  fitContent?: boolean;
}

export const TypeOnboardingWindow = forwardRef<HTMLDivElement, TypeOnboardingWindowProps>(
  function TypeOnboardingWindow({ children, className = '', fitContent = false, ...props }, ref) {
    return (
      <div className={`${styles.window} apple`} data-mini-app>
        <MotionProvider>
          <div
            ref={ref}
            className={`${styles.content} ${className}`.trim()}
            data-fit-content={fitContent || undefined}
            {...props}
          >
            {children}
          </div>
        </MotionProvider>
      </div>
    );
  },
);

export function TypeLogoHeader() {
  return (
    <div className={styles.logoHeader} aria-label="Memento">
      <Wordmark className={styles.wordmark} />
    </div>
  );
}

export function TypeStartView({ title, description }: { title: ReactNode; description?: ReactNode }) {
  return (
    <div className={styles.startView}>
      <h1 className={styles.title}>{title}</h1>
      {description ? <p className={styles.description}>{description}</p> : null}
    </div>
  );
}

export function TypeSectionList({ children }: { children: ReactNode }) {
  return <div className={styles.sectionList}>{children}</div>;
}

export function TypeSection({
  children,
  surface = 'default',
}: {
  children: ReactNode;
  surface?: 'default' | 'primary-5';
}) {
  return <section className={styles.sectionCard} data-surface={surface}>{children}</section>;
}

interface TypeCellProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'title'> {
  start?: ReactNode;
  end?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
}

export function TypeCell({ start, end, title, description, onClick, ...props }: TypeCellProps) {
  return (
    <button
      type="button"
      className={styles.cell}
      data-actionable={Boolean(onClick)}
      onClick={onClick}
      {...props}
    >
      {start ? <span className={styles.cellStart}>{start}</span> : null}
      <span className={styles.cellBody}>
        <span className={styles.cellTitle}>{title}</span>
        {description ? <span className={styles.cellDescription}>{description}</span> : null}
      </span>
      {end ? <span className={styles.cellEnd}>{end}</span> : null}
    </button>
  );
}

const avatarColors = [
  ['#ff885e', '#ff516a'],
  ['#ffcd6a', '#ffa85c'],
  ['#82b1ff', '#665fff'],
  ['#a0de7e', '#54cb68'],
  ['#53edd6', '#28c9b7'],
  ['#72d5fd', '#2a9ef1'],
  ['#e0a2f3', '#d669ed'],
] as const;

export function TypeAvatar({ children, color = 0 }: { children: ReactNode; color?: number }) {
  const [from, to] = avatarColors[color % avatarColors.length];
  return (
    <span className={styles.avatar} style={{ background: `linear-gradient(180deg, ${from}, ${to})` }}>
      {children}
    </span>
  );
}

export function TypePicker({ children, iconOnly = false }: { children: ReactNode; iconOnly?: boolean }) {
  return <span className={styles.picker} data-icon-only={iconOnly || undefined}>{children}</span>;
}

export function TypeButtonRow({ children }: { children: ReactNode }) {
  return <div className={styles.buttonRow}>{children}</div>;
}

export function TypeStepTransition({
  children,
  visible,
}: {
  children: ReactNode;
  visible: boolean;
}) {
  return (
    <div className={styles.stepTransition} data-visible={visible || undefined}>
      {children}
    </div>
  );
}

export function TypeExitCover() {
  return <div className={styles.exitCover} aria-hidden="true" />;
}

export function TypeSecondaryAction(props: ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" className={styles.secondaryAction} {...props} />;
}

export function TypeProgress({ value }: { value: number }) {
  return (
    <div className={styles.progress} aria-label={`${Math.round(value)}%`}>
      <div className={styles.progressBar} style={{ width: `${Math.max(0, Math.min(100, value))}%` }} />
    </div>
  );
}

export const typeOnboardingStyles = styles;
