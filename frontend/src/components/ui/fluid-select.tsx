"use client";

import {
  Children,
  forwardRef,
  isValidElement,
  useRef,
  useEffect,
  useState,
  useCallback,
  useMemo,
  createContext,
  useContext,
  type ReactNode,
  type HTMLAttributes,
  type ComponentType,
} from "react";
import { motion, AnimatePresence } from "framer-motion";
import { cva, type VariantProps } from "class-variance-authority";
import { Select as SelectPrimitive } from "@base-ui/react/select";
import { cn } from "@/lib/utils";
import { spring, exitFallbackMs } from "@/lib/springs";
import { useProximityHover } from "@/hooks/use-proximity-hover";
import {
  ShapeProvider,
  useShape,
  type ShapeVariant,
} from "@/lib/shape-context";
import { Elevated } from "@/lib/elevated";

type IconComponent = ComponentType<{
  size?: number;
  strokeWidth?: number;
  className?: string;
}>;

// ---------------------------------------------------------------------------
// Select context
//
// Built on Base UI's Select primitive, which owns positioning (collision
// flipping, anchor tracking), dismissal (outside press, focus-out, Escape
// nesting inside dialogs), list keyboard navigation + typeahead, combobox
// ARIA, and the hidden form input. This layer keeps the
// proximity-hover overlays, the spring open/close animation (via actionsRef
// deferred unmount), and the animated checkmark.
// ---------------------------------------------------------------------------

// How long a selection holds the popup open before closing, so the
// acknowledgment — the checkmark drawing in and the selected background
// springing to the picked row — is visible instead of being cut off by the
// ~60ms close fade. Escape and outside presses still close immediately.
const selectionAckMs = 300;

interface SelectContextValue {
  value: string;
  open: boolean;
  actionsRef: React.RefObject<{ unmount: () => void } | null>;
}

const SelectContext = createContext<SelectContextValue | null>(null);

function useSelectContext() {
  const ctx = useContext(SelectContext);
  if (!ctx) throw new Error("Select compound components must be inside <Select>");
  return ctx;
}

// Content context for proximity hover
interface SelectContentContextValue {
  registerItem: (index: number, element: HTMLElement | null) => void;
  activeIndex: number | null;
  checkedIndex?: number;
}

const SelectContentContext =
  createContext<SelectContentContextValue | null>(null);

// ---------------------------------------------------------------------------
// Select (root)
// ---------------------------------------------------------------------------

interface SelectProps {
  children: ReactNode;
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  disabled?: boolean;
  name?: string;
  required?: boolean;
  /** Pins the compound trigger and its portalled menu to one shape. */
  shape?: ShapeVariant;
}

/**
 * Walk the children tree collecting `{ value, label }` pairs from SelectItem
 * elements. Passed to Base UI's `items` prop so the trigger can resolve the
 * label of an initial value before the popup has ever mounted (items only
 * render while open). Non-string labels fall back to the raw value, matching
 * the previous labelMap behaviour.
 */
function collectSelectItems(
  node: ReactNode,
  out: { value: string; label: ReactNode }[] = []
) {
  Children.forEach(node, (child) => {
    if (!isValidElement(child)) return;
    const props = child.props as { value?: unknown; children?: ReactNode };
    if (typeof props.value === "string") {
      out.push({
        value: props.value,
        label:
          typeof props.children === "string" ? props.children : props.value,
      });
    } else if (props.children) {
      collectSelectItems(props.children, out);
    }
  });
  return out;
}

function Select({
  children,
  value,
  defaultValue,
  onValueChange,
  disabled = false,
  name,
  required,
  shape,
}: SelectProps) {
  const [internalValue, setInternalValue] = useState(defaultValue ?? "");
  const [open, setOpen] = useState(false);
  const actionsRef = useRef<{ unmount: () => void } | null>(null);
  const currentValue = value !== undefined ? value : internalValue;

  const items = useMemo(() => collectSelectItems(children), [children]);

  const handleValueChange = useCallback(
    (next: string | null) => {
      const v = next ?? "";
      if (value === undefined) setInternalValue(v);
      onValueChange?.(v);
    },
    [value, onValueChange]
  );

  const ackTimeoutRef = useRef<number | null>(null);
  const cancelAckClose = useCallback(() => {
    if (ackTimeoutRef.current !== null) {
      clearTimeout(ackTimeoutRef.current);
      ackTimeoutRef.current = null;
    }
  }, []);
  useEffect(() => cancelAckClose, [cancelAckClose]);

  // Picking an item acknowledges before closing: the close is deferred by
  // selectionAckMs so the checkmark draw and the selected background's spring
  // to the picked row are seen. Every other close reason (Escape, outside
  // press, trigger toggle, focus-out) closes immediately and cancels any
  // pending acknowledgment; re-picking within the window restarts it.
  const handleOpenChange = useCallback(
    (nextOpen: boolean, eventDetails: { reason: string }) => {
      if (!nextOpen && eventDetails.reason === "item-press") {
        cancelAckClose();
        ackTimeoutRef.current = window.setTimeout(() => {
          ackTimeoutRef.current = null;
          setOpen(false);
        }, selectionAckMs);
        return;
      }
      cancelAckClose();
      setOpen(nextOpen);
    },
    [cancelAckClose]
  );

  const ctx = useMemo(
    () => ({ value: currentValue, open, actionsRef }),
    [currentValue, open]
  );

  const root = (
    <SelectContext.Provider value={ctx}>
      <SelectPrimitive.Root
        // Always controlled; "" (no selection) maps to Base UI's null.
        value={currentValue === "" ? null : currentValue}
        onValueChange={handleValueChange}
        open={open}
        onOpenChange={handleOpenChange}
        actionsRef={actionsRef}
        items={items}
        disabled={disabled}
        name={name}
        required={required}
        // Non-modal: the page keeps scrolling and the Positioner tracks the
        // anchor, so the popup follows its trigger instead of detaching.
        modal={false}
      >
        {children}
      </SelectPrimitive.Root>
    </SelectContext.Provider>
  );

  return shape ? (
    <ShapeProvider defaultShape={shape} publishToRoot={false}>
      {root}
    </ShapeProvider>
  ) : (
    root
  );
}

Select.displayName = "Select";

// ---------------------------------------------------------------------------
// SelectTrigger
// ---------------------------------------------------------------------------

const triggerVariants = cva(
  [
    "group inline-flex items-center justify-between gap-2 outline-none cursor-pointer",
    "text-[13px] h-9 px-3 min-w-[160px]",
    "transition-all duration-80",
    "disabled:opacity-50 disabled:pointer-events-none",
    "focus-visible:ring-1 focus-visible:ring-[color:var(--focus-ring,#6B97FF)]",
  ],
  {
    variants: {
      variant: {
        bordered:
          "fluid-select-token-surface fluid-select-token-trigger border border-[var(--primary-10)] bg-[var(--elevation-2)] text-[var(--deslop-primary)]",
        borderless:
          "border border-transparent bg-transparent text-[var(--deslop-primary)] hover:bg-[var(--primary-5)]",
      },
    },
    defaultVariants: {
      variant: "bordered",
    },
  }
);

interface SelectTriggerProps
  extends Omit<HTMLAttributes<HTMLButtonElement>, "children">,
    VariantProps<typeof triggerVariants> {
  icon?: IconComponent;
  placeholder?: string;
  error?: string;
}

const SelectTrigger = forwardRef<HTMLButtonElement, SelectTriggerProps>(
  (
    { className, variant, icon: Icon, placeholder = "Select…", error, ...props },
    ref
  ) => {
    const shape = useShape();

    return (
      <div className="flex flex-col gap-1">
        <SelectPrimitive.Trigger
          ref={ref}
          aria-invalid={!!error || undefined}
          className={cn(
            triggerVariants({ variant }),
            shape.input,
            error && "border-destructive/50 hover:border-destructive/50",
            className
          )}
          {...props}
        >
          <span className="flex items-center gap-2 min-w-0 flex-1">
            {Icon && (
              <Icon
                size={16}
                strokeWidth={1.5}
                className="shrink-0 text-muted-foreground transition-[color,stroke-width] duration-80 group-hover:text-foreground group-hover:stroke-[2]"
              />
            )}
            <SelectPrimitive.Value
              placeholder={placeholder}
              // py-1/-my-1: truncate's overflow:hidden clips at the padding
              // box, and the trimmed box excludes ascenders/descenders — the
              // padding gives glyphs room while the negative margin keeps the
              // trimmed layout box.
              className="min-w-0 flex-1 text-left truncate [text-box:trim-both_cap_alphabetic] py-1 -my-1 data-[placeholder]:text-muted-foreground"
            />
          </span>

          <svg
            width={16}
            height={16}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            className="shrink-0 text-muted-foreground transition-colors duration-80 group-hover:text-foreground"
          >
            <path d="M6 9l6 6 6-6" />
          </svg>
        </SelectPrimitive.Trigger>
        {error && (
          <span className="text-[12px] text-destructive pl-3">{error}</span>
        )}
      </div>
    );
  }
);

SelectTrigger.displayName = "SelectTrigger";

// ---------------------------------------------------------------------------
// SelectContent
// ---------------------------------------------------------------------------

interface SelectContentProps {
  className?: string;
  children: ReactNode;
}

const SelectContent = forwardRef<HTMLDivElement, SelectContentProps>(
  ({ className, children }, ref) => {
    const { open, value, actionsRef } = useSelectContext();
    const shape = useShape();
    const containerRef = useRef<HTMLDivElement>(null);

    const {
      activeIndex,
      setActiveIndex,
      itemRects,
      isMeasured,
      sessionRef,
      handlers,
      registerItem,
      remeasure,
    } = useProximityHover(containerRef);

    const [focusedIndex, setFocusedIndex] = useState<number | null>(null);
    const [checkedIndex, setCheckedIndex] = useState<number | undefined>(
      undefined
    );

    // Release Base UI's deferred unmount once the exit tween has played.
    // onAnimationComplete on the motion.div is the primary signal; this
    // timeout is a fallback for throttled/background tabs where rAF-driven
    // animation callbacks can stall. The popup exits with spring.fast, so the
    // fallback tracks that tier's exit duration plus a safety buffer.
    useEffect(() => {
      if (open) return;
      const id = setTimeout(
        () => actionsRef.current?.unmount(),
        exitFallbackMs(spring.fast)
      );
      return () => clearTimeout(id);
    }, [open, actionsRef]);

    // Fresh rects once per open. Measuring is the hook's job — it owns the
    // one coalesced pass that item registration and container resizes both
    // feed into, and a second pass from elsewhere is what used to land a
    // corrected rect on an already-mounted overlay. The popup keeps its items
    // registered while it sits hidden between opens, so registration alone
    // would never trigger a fresh pass on reopen.
    useEffect(() => {
      if (!open) return;
      remeasure();
    }, [open, remeasure]);

    // Detect the checked row. Deliberately does NOT remeasure on a value
    // change while open: the rows haven't moved, so the published rects stay
    // trustworthy and only checkedIndex switches — which lets the selected
    // marker spring from the old row to the picked one (the selection
    // acknowledgment) instead of unmounting and snapping.
    useEffect(() => {
      if (!open) return;
      // Double rAF: first waits for React commit, second for layout
      let inner: number;
      const outer = requestAnimationFrame(() => {
        inner = requestAnimationFrame(() => {
          const container = containerRef.current;
          if (container) {
            const items = Array.from(
              container.querySelectorAll("[data-proximity-index]")
            ) as HTMLElement[];
            const idx = items.findIndex(
              (el) => el.getAttribute("data-value") === value
            );
            setCheckedIndex(idx !== -1 ? idx : undefined);
          }
        });
      });
      return () => {
        cancelAnimationFrame(outer);
        cancelAnimationFrame(inner);
      };
    }, [open, value]);

    // Reset every overlay index as the close begins. checkedIndex otherwise
    // lags one open behind value (picking an item closes the popup before the
    // effect above re-syncs it), and a leftover activeIndex is worse: Base UI
    // keeps the popup mounted through the exit tween, so on reopen the hover
    // pill would still be sitting on the previously active row and spring from
    // there to the row that auto-focus lands on.
    useEffect(() => {
      if (open) return;
      setCheckedIndex(undefined);
      setActiveIndex(null);
      setFocusedIndex(null);
    }, [open, setActiveIndex]);

    // Overlays read rects only once the hook reports the item set fully
    // measured. Positioning one from an incomplete pass mounts it at the wrong
    // row, and the correcting pass then springs it across the list.
    const activeRect =
      isMeasured && activeIndex !== null ? itemRects[activeIndex] : null;
    const checkedRect =
      isMeasured && checkedIndex != null ? itemRects[checkedIndex] : null;
    const focusRect =
      isMeasured && focusedIndex !== null ? itemRects[focusedIndex] : null;

    const contentCtx = useMemo(
      () => ({ registerItem, activeIndex, checkedIndex }),
      [registerItem, activeIndex, checkedIndex]
    );

    return (
      <SelectPrimitive.Portal>
        <SelectPrimitive.Positioner
          side="bottom"
          align="start"
          sideOffset={6}
          alignItemWithTrigger={false}
          className="z-50 outline-none"
        >
          <motion.div
            initial={{ opacity: 0, y: -4, scaleY: 0.96 }}
            animate={
              open
                ? { opacity: 1, y: 0, scaleY: 1 }
                : { opacity: 0, y: -4, scaleY: 0.96 }
            }
            transition={open ? spring.fast : spring.fast.exit}
            style={{ transformOrigin: "top center" }}
            // Base UI defers unmount while actionsRef is set; release it once
            // the exit spring has finished so the close animation fully plays.
            onAnimationComplete={() => {
              if (!open) actionsRef.current?.unmount();
            }}
          >
            <SelectContentContext.Provider value={contentCtx}>
              <SelectPrimitive.Popup
                render={
                  <Elevated
                    offset={2}
                    shadowLevel={3}
                    ref={(node: HTMLDivElement | null) => {
                      (
                        containerRef as React.MutableRefObject<HTMLDivElement | null>
                      ).current = node;
                      if (typeof ref === "function") ref(node);
                      else if (ref)
                        (
                          ref as React.MutableRefObject<HTMLDivElement | null>
                        ).current = node;
                    }}
                  />
                }
                onMouseEnter={() => {
                  handlers.onMouseEnter();
                  setFocusedIndex(null);
                }}
                onMouseMove={handlers.onMouseMove}
                onMouseLeave={handlers.onMouseLeave}
                onFocus={(e) => {
                  const indexAttr = (e.target as HTMLElement)
                    .closest("[data-proximity-index]")
                    ?.getAttribute("data-proximity-index");
                  if (indexAttr != null) {
                    const idx = Number(indexAttr);
                    setActiveIndex(idx);
                    setFocusedIndex(
                      (e.target as HTMLElement).matches(":focus-visible")
                        ? idx
                        : null
                    );
                  }
                }}
                onBlur={(e) => {
                  if (containerRef.current?.contains(e.relatedTarget as Node))
                    return;
                  setFocusedIndex(null);
                  setActiveIndex(null);
                }}
                className={cn(
                  // min-w tracks the trigger via the Positioner's --anchor-width
                  // var, matching the pre-migration minWidth: triggerRect.width.
                  `fluid-select-token-surface relative flex flex-col gap-0.5 min-w-[var(--anchor-width)] max-h-[min(300px,var(--available-height))] overflow-y-auto ${shape.container} p-1 select-none outline-none`,
                  className
                )}
              >
                {/* The three overlays are torn down as the close begins rather
                    than exit-animated, because an overlay still mounted when the
                    popup reopens is one AnimatePresence re-adopts under its old
                    key: `initial` never runs again, so it keeps the position of the
                    row it had before and animates from there to the new one. The
                    popup's own fade covers their disappearance. */}
                {/* Selected background */}
                {open && (
                  <AnimatePresence>
                    {checkedRect && (
                      <motion.div
                        className={`absolute ${shape.bg} bg-[var(--primary-5)] pointer-events-none`}
                        // Position lives in `animate` so an in-session value
                        // change springs the marker to the picked row (the
                        // selection acknowledgment). Safe against the reopen
                        // slide: the `open &&` teardown means no marker
                        // survives a close, and a fresh mount with
                        // initial={false} renders snapped at these values.
                        initial={false}
                        animate={{
                          top: checkedRect.top,
                          left: checkedRect.left,
                          width: checkedRect.width,
                          height: checkedRect.height,
                          opacity: 1,
                        }}
                        exit={{ opacity: 0, transition: spring.moderate.exit }}
                        transition={{
                          ...spring.moderate,
                          opacity: { duration: 0.08 },
                        }}
                      />
                    )}
                  </AnimatePresence>
                )}

                {/* Hover background */}
                {open && (
                  <AnimatePresence>
                    {activeRect && (
                      <motion.div
                        key={sessionRef.current}
                        className={`absolute ${shape.bg} bg-[var(--primary-5)] pointer-events-none`}
                        initial={{
                          opacity: 0,
                          top: activeRect.top,
                          left: activeRect.left,
                          width: activeRect.width,
                          height: activeRect.height,
                        }}
                        animate={{
                          opacity: 1,
                          top: activeRect.top,
                          left: activeRect.left,
                          width: activeRect.width,
                          height: activeRect.height,
                        }}
                        exit={{ opacity: 0, transition: spring.fast.exit }}
                        transition={{
                          ...spring.fast,
                          opacity: { duration: 0.08 },
                        }}
                      />
                    )}
                  </AnimatePresence>
                )}

                {/* Focus ring */}
                {open && (
                  <AnimatePresence>
                    {focusRect && (
                      <motion.div
                        className={`absolute ${shape.focusRing} pointer-events-none z-20 border border-[color:var(--focus-ring,#6B97FF)]`}
                        initial={false}
                        animate={{
                          left: focusRect.left - 2,
                          top: focusRect.top - 2,
                          width: focusRect.width + 4,
                          height: focusRect.height + 4,
                        }}
                        exit={{ opacity: 0, transition: spring.fast.exit }}
                        transition={{
                          ...spring.fast,
                          opacity: { duration: 0.08 },
                        }}
                      />
                    )}
                  </AnimatePresence>
                )}

                {children}
              </SelectPrimitive.Popup>
            </SelectContentContext.Provider>
          </motion.div>
        </SelectPrimitive.Positioner>
      </SelectPrimitive.Portal>
    );
  }
);

SelectContent.displayName = "SelectContent";

// ---------------------------------------------------------------------------
// SelectItem
// ---------------------------------------------------------------------------

interface SelectItemProps extends HTMLAttributes<HTMLDivElement> {
  icon?: IconComponent;
  index: number;
  value: string;
  disabled?: boolean;
}

const SelectItem = forwardRef<HTMLDivElement, SelectItemProps>(
  (
    {
      className,
      children,
      icon: Icon,
      value,
      index,
      disabled = false,
      ...props
    },
    ref
  ) => {
    const selectCtx = useSelectContext();
    const contentCtx = useContext(SelectContentContext);
    const internalRef = useRef<HTMLDivElement>(null);
    const shape = useShape();
    const hasMounted = useRef(false);

    useEffect(() => {
      hasMounted.current = true;
    }, []);

    // Register with proximity hover. Depends on the (stable) registerItem
    // rather than the content context, which is rebuilt on every activeIndex
    // change: keying the effect to the whole context re-ran it per mousemove,
    // unregistering and re-registering every row and so keeping the hook's
    // measurement permanently unsettled while the pointer moved.
    const registerItem = contentCtx?.registerItem;
    useEffect(() => {
      if (!registerItem) return;
      registerItem(index, internalRef.current);
      return () => registerItem(index, null);
    }, [index, registerItem]);

    const isActive = contentCtx?.activeIndex === index;
    const isChecked = selectCtx.value === value;
    const skipAnimation = !hasMounted.current;

    return (
      <SelectPrimitive.Item
        value={value}
        disabled={disabled}
        label={typeof children === "string" ? children : undefined}
        render={
          <div
            ref={(node: HTMLDivElement | null) => {
              (
                internalRef as React.MutableRefObject<HTMLDivElement | null>
              ).current = node;
              if (typeof ref === "function") ref(node);
              else if (ref)
                (ref as React.MutableRefObject<HTMLDivElement | null>).current =
                  node;
            }}
            data-proximity-index={index}
            data-value={value}
            className={cn(
              // Fixed height (was py-2 around a 19.5px line box ≈ 35.5px) so
              // the text-box trim on the item text doesn't shrink the row.
              // shrink-0: the popup is a max-height flex column, so without it
              // a long list compresses rows to fit instead of scrolling.
              `relative z-10 flex h-9 shrink-0 items-center gap-2 ${shape.item} px-2 text-[13px] cursor-pointer outline-none select-none`,
              "transition-[color] duration-80",
              isActive || isChecked
                ? "text-[var(--deslop-primary)]"
                : "text-[var(--deslop-primary-60)]",
              disabled && "opacity-50 pointer-events-none",
              className
            )}
            {...props}
          />
        }
      >
        {Icon && (
          <Icon
            size={16}
            strokeWidth={isActive || isChecked ? 2 : 1.5}
            className="shrink-0 transition-[color,stroke-width] duration-80"
          />
        )}

        <SelectPrimitive.ItemText
          // py-1/-my-1 keeps truncate's overflow:hidden from clipping
          // ascenders/descenders outside the trimmed box.
          render={<span className="flex-1 min-w-0 truncate [text-box:trim-both_cap_alphabetic] py-1 -my-1" />}
        >
          {children}
        </SelectPrimitive.ItemText>

        {/* Always-rendered fixed slot so the check appearing/disappearing
            never changes the row's intrinsic width — without it the whole
            popup resizes when a selection lands. */}
        <span aria-hidden className="shrink-0 w-4 h-4">
          <AnimatePresence>
            {isChecked && (
              <motion.svg
                key="check"
                width={16}
                height={16}
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                className="text-foreground"
                initial={{ opacity: 1 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 1 }}
              >
                <motion.path
                  d="M4 12L9 17L20 6"
                  initial={{ pathLength: skipAnimation ? 1 : 0 }}
                  animate={{
                    pathLength: 1,
                    transition: { duration: 0.08, ease: "easeOut" },
                  }}
                  exit={{
                    pathLength: 0,
                    transition: { duration: 0.04, ease: "easeIn" },
                  }}
                />
              </motion.svg>
            )}
          </AnimatePresence>
        </span>
      </SelectPrimitive.Item>
    );
  }
);

SelectItem.displayName = "SelectItem";

// ---------------------------------------------------------------------------
// SelectGroup + SelectLabel + SelectSeparator
// ---------------------------------------------------------------------------

function SelectGroup({
  children,
  className,
  ...props
}: HTMLAttributes<HTMLDivElement>) {
  return (
    <div role="group" className={className} {...props}>
      {children}
    </div>
  );
}

SelectGroup.displayName = "SelectGroup";

const SelectLabel = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        "px-2 py-1.5 shrink-0 text-[11px] text-muted-foreground",
        className
      )}
      {...props}
    />
  )
);

SelectLabel.displayName = "SelectLabel";

const SelectSeparator = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    role="separator"
    className={cn("my-1 -mx-1 h-px shrink-0 bg-[var(--primary-10)]", className)}
    {...props}
  />
));

SelectSeparator.displayName = "SelectSeparator";

// ---------------------------------------------------------------------------
// Exports
// ---------------------------------------------------------------------------

export {
  Select,
  SelectTrigger,
  SelectContent,
  SelectItem,
  SelectGroup,
  SelectLabel,
  SelectSeparator,
  triggerVariants,
};

export type { SelectProps, SelectTriggerProps, SelectContentProps, SelectItemProps };
