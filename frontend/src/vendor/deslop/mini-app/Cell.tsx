import {
  forwardRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type ReactNode,
  type Ref,
} from 'react';

import styles from './Cell.module.css';

interface CellOwnProps {
  start?: ReactNode;
  end?: ReactNode;
  children?: ReactNode;
  className?: string;
}

/**
 * A cell renders as a `<button>` by default. `as="div"` is for cells that carry their own
 * interactive children — a button inside a button is invalid HTML — and it only accepts div
 * attributes, so button-only props (`type`, `disabled`) fail to compile instead of reaching
 * the DOM as stray attributes.
 */
type CellProps =
  | (CellOwnProps & Omit<ButtonHTMLAttributes<HTMLButtonElement>, keyof CellOwnProps> & { as?: 'button' })
  | (CellOwnProps & Omit<HTMLAttributes<HTMLDivElement>, keyof CellOwnProps> & { as: 'div' });

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
const CellRoot = forwardRef<HTMLElement, CellProps>(function Cell(props, ref) {
  const { start, end, children, className = '' } = props;
  const content = (
    <>
      {start ? <span className={styles.start}>{start}</span> : null}
      <span className={styles.body}>{children}</span>
      {end ? <span className={styles.end}>{end}</span> : null}
    </>
  );

  if (props.as === 'div') {
    const { as: _as, start: _start, end: _end, children: _children, className: _className, ...divProps } = props;
    return (
      <div ref={ref as Ref<HTMLDivElement>} className={`${styles.root} ${className}`} {...divProps}>
        {content}
      </div>
    );
  }

  const { as: _as, start: _start, end: _end, children: _children, className: _className, ...buttonProps } = props;
  return (
    <button ref={ref as Ref<HTMLButtonElement>} className={`${styles.root} ${className}`} {...buttonProps}>
      {content}
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
