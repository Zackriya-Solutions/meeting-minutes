import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';

import styles from './Cell.module.css';

interface CellProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  start?: ReactNode;
  end?: ReactNode;
}

interface CellTextProps {
  title: ReactNode;
  description?: ReactNode;
  bold?: boolean;
}

/**
 * Desktop TypeScript port of mishanaer/deslop mini-app/components/Cells.
 * The source package is not currently published, so Memento vendors the
 * component beside the Deslop primitives it already consumes.
 */
const CellRoot = forwardRef<HTMLButtonElement, CellProps>(function Cell(
  { start, end, children, className = '', ...props },
  ref,
) {
  return (
    <button ref={ref} className={`${styles.root} ${className}`} {...props}>
      {start ? <span className={styles.start}>{start}</span> : null}
      <span className={styles.body}>{children}</span>
      {end ? <span className={styles.end}>{end}</span> : null}
    </button>
  );
});

export function CellText({ title, description, bold = false }: CellTextProps) {
  return (
    <span className={styles.text}>
      <span className={styles.label} data-weight={bold ? 'medium' : 'regular'}>
        {typeof title === 'string' ? (
          <span className={styles.ellipsis}>{title}</span>
        ) : title}
      </span>
      {description ? <span className={styles.caption}>{description}</span> : null}
    </span>
  );
}

export const Cell = Object.assign(CellRoot, { Text: CellText });
export default Cell;
