"use client";

import {
  useRef,
  useState,
  useEffect,
  useCallback,
  useMemo,
  createContext,
  useContext,
  forwardRef,
  cloneElement,
  type ReactNode,
  type ReactElement,
  type HTMLAttributes,
  type ComponentPropsWithoutRef,
} from "react";
import { motion, AnimatePresence } from "framer-motion";
import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import { cn } from "@/lib/utils";
import { spring, exitFallbackMs } from "@/lib/springs";
import { useProximityHover } from "@/hooks/use-proximity-hover";
import { shapeMap } from "@/lib/shape-context";
import { Elevated } from "@/lib/elevated";
import {
  DropdownContext,
  useDropdown,
  useDropdownMaybe,
  type DropdownContextValue,
  type MenuItemRenderOptions,
} from "@/components/ui/menu-item";

// Dropdown opts out of the global pill/rounded shape context — popover surfaces
// look cleaner with the smaller "rounded" radii regardless of how the rest of
// the UI is shaped (the heavy pill bubbling distorts perceived padding at this
// scale and produces the corner-shadow asymmetry).
const shape = shapeMap.rounded;

// ---------------------------------------------------------------------------
// Panel context — shared by the inline Dropdown and the popup DropdownContent.
//
// The context object itself lives in menu-item.tsx so MenuItem resolves
// whichever dropdown provider actually wraps it, even when dropdowns built
// on different primitives render side by side. Re-exported here so the
// public dropdown API is unchanged.
// ---------------------------------------------------------------------------

export { useDropdown, useDropdownMaybe };
export type { DropdownContextValue, MenuItemRenderOptions };

// ---------------------------------------------------------------------------
// Dropdown (inline panel)
//
// An always-rendered panel — no trigger, positioning, or dismissal. Because it
// sits statically in the page it does NOT claim popup menu semantics: the
// container is a plain role="group" (pass `aria-label` to name it). The real
// role="menu" lives on the popup DropdownContent below, which Radix wires to
// a trigger. Consumers who hand-roll a trigger around the inline panel get
// grouping semantics rather than a falsely-announced popup menu.
// ---------------------------------------------------------------------------

interface DropdownProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
  checkedIndex?: number;
}

const Dropdown = forwardRef<HTMLDivElement, DropdownProps>(
  ({ children, checkedIndex, className, ...props }, ref) => {
    const containerRef = useRef<HTMLDivElement>(null);
    const {
      activeIndex,
      setActiveIndex,
      itemRects,
      sessionRef,
      handlers,
      registerItem,
      measureItems,
    } = useProximityHover(containerRef);

    useEffect(() => {
      measureItems();
    }, [measureItems, children]);

    const [focusedIndex, setFocusedIndex] = useState<number | null>(null);

    const activeRect = activeIndex !== null ? itemRects[activeIndex] : null;
    const checkedRect =
      checkedIndex != null ? itemRects[checkedIndex] : null;
    const focusRect = focusedIndex !== null ? itemRects[focusedIndex] : null;
    return (
      <DropdownContext.Provider value={{ registerItem, activeIndex, checkedIndex }}>
        <Elevated
          offset={2}
          shadowLevel={3}
          ref={(node) => {
            (containerRef as React.MutableRefObject<HTMLDivElement | null>).current = node;
            if (typeof ref === "function") ref(node);
            else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
          }}
          onMouseEnter={handlers.onMouseEnter}
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
                (e.target as HTMLElement).matches(":focus-visible") ? idx : null
              );
            }
          }}
          onBlur={(e) => {
            if (containerRef.current?.contains(e.relatedTarget as Node)) return;
            setFocusedIndex(null);
            setActiveIndex(null);
          }}
          onKeyDown={(e) => {
            const items = Array.from(
              containerRef.current?.querySelectorAll(
                '[role="menuitem"], [role="menuitemradio"]'
              ) ?? []
            ) as HTMLElement[];
            const currentIdx = items.indexOf(e.target as HTMLElement);
            if (currentIdx === -1) return;

            if (["ArrowDown", "ArrowUp", "ArrowRight", "ArrowLeft"].includes(e.key)) {
              e.preventDefault();
              const next = ["ArrowDown", "ArrowRight"].includes(e.key)
                ? (currentIdx + 1) % items.length
                : (currentIdx - 1 + items.length) % items.length;
              items[next].focus();
            } else if (e.key === "Home") {
              e.preventDefault();
              items[0]?.focus();
            } else if (e.key === "End") {
              e.preventDefault();
              items[items.length - 1]?.focus();
            }
          }}
          role="group"
          className={cn(
            `relative flex flex-col gap-0.5 w-72 max-w-full ${shape.container} p-1 select-none`,
            className
          )}
          {...props}
        >
          {/* Selected background */}
          <AnimatePresence>
            {checkedRect && (
              <motion.div
                className={`absolute ${shape.bg} bg-active pointer-events-none`}
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

          {/* Hover background */}
          <AnimatePresence>
            {activeRect && (
              <motion.div
                key={sessionRef.current}
                className={`absolute ${shape.bg} bg-hover pointer-events-none`}
                initial={{
                  opacity: 0,
                  top: checkedRect?.top ?? activeRect.top,
                  left: checkedRect?.left ?? activeRect.left,
                  width: checkedRect?.width ?? activeRect.width,
                  height: checkedRect?.height ?? activeRect.height,
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

          {/* Focus ring */}
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

          {children}
        </Elevated>
      </DropdownContext.Provider>
    );
  }
);

Dropdown.displayName = "Dropdown";

// ---------------------------------------------------------------------------
// DropdownMenu (popup root)
//
// Built on Radix's DropdownMenu primitive, which owns the trigger wiring,
// positioning (collision flipping, anchor tracking), dismissal (outside
// press, focus-out, Escape), roving highlight, typeahead, and close-on-select.
// This layer keeps the proximity-hover overlays and the
// spring open/close animation. Radix has no actionsRef-style deferred unmount,
// so the portal lifetime is managed with local `mounted` state (the same
// pattern the Dialog and MobileDrawer components use).
// ---------------------------------------------------------------------------

interface DropdownMenuContextValue {
  open: boolean;
  disabled: boolean;
}

const DropdownMenuContext = createContext<DropdownMenuContextValue | null>(null);

function useDropdownMenuContext() {
  const ctx = useContext(DropdownMenuContext);
  if (!ctx)
    throw new Error(
      "DropdownMenu compound components must be inside <DropdownMenu>"
    );
  return ctx;
}

interface DropdownMenuProps {
  children: ReactNode;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  disabled?: boolean;
}

function DropdownMenu({
  children,
  open: openProp,
  defaultOpen = false,
  onOpenChange,
  disabled = false,
}: DropdownMenuProps) {
  const [internalOpen, setInternalOpen] = useState(defaultOpen);
  const open = openProp !== undefined ? openProp : internalOpen;

  const handleOpenChange = useCallback(
    (next: boolean) => {
      if (openProp === undefined) setInternalOpen(next);
      onOpenChange?.(next);
    },
    [openProp, onOpenChange]
  );

  const ctx = useMemo(() => ({ open, disabled }), [open, disabled]);

  return (
    <DropdownMenuContext.Provider value={ctx}>
      {/* Root is always controlled by `open` (defaultOpen seeds local state
          instead of being forwarded), so DropdownContent can drive the exit
          animation before the portal unmounts. Non-modal: the page keeps
          scrolling and the popup tracks its anchor instead of detaching. */}
      <DropdownMenuPrimitive.Root
        open={open}
        onOpenChange={handleOpenChange}
        modal={false}
      >
        {children}
      </DropdownMenuPrimitive.Root>
    </DropdownMenuContext.Provider>
  );
}

DropdownMenu.displayName = "DropdownMenu";

// ---------------------------------------------------------------------------
// DropdownTrigger
//
// Radix's DropdownMenu.Trigger behind a Base-UI-style `render` prop, so
// any element can be the trigger with the same public API:
//
//   <DropdownTrigger render={<Button variant="secondary">Open</Button>} />
//
// `render` maps to Radix's asChild composition. Root-level `disabled` parity
// with Base UI's Menu.Root: the flag flows through context onto the trigger.
// ---------------------------------------------------------------------------

interface DropdownTriggerProps
  extends Omit<
    ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Trigger>,
    "asChild"
  > {
  /** Element to render as the trigger (Base-UI-style `render` composition
   *  API). */
  render?: ReactElement;
}

const DropdownTrigger = forwardRef<HTMLButtonElement, DropdownTriggerProps>(
  ({ render, children, disabled, ...props }, ref) => {
    const { disabled: rootDisabled } = useDropdownMenuContext();
    const isDisabled = disabled || rootDisabled;

    if (render) {
      return (
        <DropdownMenuPrimitive.Trigger
          ref={ref}
          asChild
          disabled={isDisabled}
          {...props}
        >
          {render}
        </DropdownMenuPrimitive.Trigger>
      );
    }
    return (
      <DropdownMenuPrimitive.Trigger ref={ref} disabled={isDisabled} {...props}>
        {children}
      </DropdownMenuPrimitive.Trigger>
    );
  }
);

DropdownTrigger.displayName = "DropdownTrigger";

// ---------------------------------------------------------------------------
// DropdownContent (popup panel)
//
// Portal > Content carrying the exact inline-panel visuals: Elevated surface,
// proximity-hover overlays, animated selected background, and animated focus
// ring. Children are wrapped in a RadioGroup so radio-style MenuItems
// (boolean `checked`) get correct aria-checked from `checkedIndex` (Radix
// radio values are strings, so the index maps through String()).
// ---------------------------------------------------------------------------

type RadixContentProps = ComponentPropsWithoutRef<
  typeof DropdownMenuPrimitive.Content
>;

interface DropdownContentProps {
  children: ReactNode;
  className?: string;
  /** Index of the checked item. Drives the animated selected background and
   *  the radio-group value announced to assistive tech. */
  checkedIndex?: number;
  side?: RadixContentProps["side"];
  align?: RadixContentProps["align"];
  sideOffset?: number;
}

const DropdownContent = forwardRef<HTMLDivElement, DropdownContentProps>(
  (
    {
      className,
      children,
      checkedIndex,
      side = "bottom",
      align = "start",
      sideOffset = 6,
    },
    ref
  ) => {
    const { open } = useDropdownMenuContext();
    const containerRef = useRef<HTMLDivElement>(null);

    const {
      activeIndex,
      setActiveIndex,
      itemRects,
      sessionRef,
      handlers,
      registerItem,
      measureItems,
    } = useProximityHover(containerRef);

    const [focusedIndex, setFocusedIndex] = useState<number | null>(null);

    // Portal lifetime: mounts as soon as `open` flips true; on close it stays
    // mounted (forceMount below) until the exit tween finishes.
    const [mounted, setMounted] = useState(false);

    useEffect(() => {
      if (open) setMounted(true);
    }, [open]);

    // Fallback release for the deferred unmount: onAnimationComplete on the
    // motion.div is the primary signal, but rAF-driven animation callbacks
    // can stall in throttled/background tabs. The popup exits with
    // spring.fast, so the fallback tracks that tier's exit duration plus a
    // safety buffer.
    useEffect(() => {
      if (open) return;
      const id = setTimeout(() => setMounted(false), exitFallbackMs(spring.fast));
      return () => clearTimeout(id);
    }, [open]);

    // Measure items once the popup has mounted.
    useEffect(() => {
      if (!open || !mounted) return;
      // Double rAF: first waits for React commit, second for layout
      let inner: number;
      const outer = requestAnimationFrame(() => {
        inner = requestAnimationFrame(() => {
          measureItems();
        });
      });
      return () => {
        cancelAnimationFrame(outer);
        cancelAnimationFrame(inner);
      };
    }, [open, mounted, measureItems]);

    const activeRect = activeIndex !== null ? itemRects[activeIndex] : null;
    const checkedRect = checkedIndex != null ? itemRects[checkedIndex] : null;
    const focusRect = focusedIndex !== null ? itemRects[focusedIndex] : null;
    // Inside the popup, Radix's Item / RadioItem own the role, aria-checked,
    // tabIndex, roving highlight, typeahead, and Enter/Space/click activation
    // (keyboard activation synthesizes a click, so the row div's onClick also
    // fires for keyboard). The styled div composes via asChild, with the row
    // content cloned back in as its children.
    const renderMenuItem = useCallback(
      ({
        radio,
        value,
        disabled,
        label,
        closeOnClick,
        element,
        children,
      }: MenuItemRenderOptions) => {
        const commonProps = {
          asChild: true,
          disabled,
          textValue: label,
          // Radix closes the menu on select by default; preventing the select
          // event keeps it open — Base UI's closeOnClick={false} parity.
          onSelect: closeOnClick
            ? undefined
            : (event: Event) => event.preventDefault(),
        };
        const item = cloneElement(element, {}, children);
        return radio ? (
          <DropdownMenuPrimitive.RadioItem value={String(value)} {...commonProps}>
            {item}
          </DropdownMenuPrimitive.RadioItem>
        ) : (
          <DropdownMenuPrimitive.Item {...commonProps}>
            {item}
          </DropdownMenuPrimitive.Item>
        );
      },
      []
    );

    const contentCtx = useMemo(
      () => ({
        registerItem,
        activeIndex,
        checkedIndex,
        inMenu: true,
        renderMenuItem,
      }),
      [registerItem, activeIndex, checkedIndex, renderMenuItem]
    );

    if (!mounted) return null;

    return (
      <DropdownMenuPrimitive.Portal forceMount>
        <DropdownMenuPrimitive.Content
          asChild
          forceMount
          side={side}
          align={align}
          sideOffset={sideOffset}
        >
          <motion.div
            className="z-50 outline-none"
            initial={{ opacity: 0, y: -4, scaleY: 0.96 }}
            animate={
              open
                ? { opacity: 1, y: 0, scaleY: 1 }
                : { opacity: 0, y: -4, scaleY: 0.96 }
            }
            transition={open ? spring.fast : spring.fast.exit}
            style={{ transformOrigin: "top center" }}
            // Release the deferred unmount once the exit spring has finished
            // so the close animation fully plays.
            onAnimationComplete={() => {
              if (!open) setMounted(false);
            }}
          >
            <DropdownContext.Provider value={contentCtx}>
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
                  // min-w tracks the trigger; the available-height guard maps
                  // Base UI's --available-height to Radix's equivalent var.
                  `relative flex flex-col gap-0.5 w-72 max-w-full min-w-[var(--radix-dropdown-menu-trigger-width)] max-h-[min(480px,var(--radix-dropdown-menu-content-available-height))] overflow-y-auto ${shape.container} p-1 select-none outline-none`,
                  className
                )}
              >
                {/* Selected background */}
                <AnimatePresence>
                  {checkedRect && (
                    <motion.div
                      className={`absolute ${shape.bg} bg-active pointer-events-none`}
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

                {/* Hover background */}
                <AnimatePresence>
                  {activeRect && (
                    <motion.div
                      key={sessionRef.current}
                      className={`absolute ${shape.bg} bg-hover pointer-events-none`}
                      initial={{
                        opacity: 0,
                        top: checkedRect?.top ?? activeRect.top,
                        left: checkedRect?.left ?? activeRect.left,
                        width: checkedRect?.width ?? activeRect.width,
                        height: checkedRect?.height ?? activeRect.height,
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

                {/* Focus ring */}
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

                {/* display: contents keeps items direct flex children of the
                    panel so proximity measurement and gap layout still work,
                    while the group provides the radio value context. */}
                <DropdownMenuPrimitive.RadioGroup
                  value={checkedIndex != null ? String(checkedIndex) : undefined}
                  className="contents"
                >
                  {children}
                </DropdownMenuPrimitive.RadioGroup>
              </Elevated>
            </DropdownContext.Provider>
          </motion.div>
        </DropdownMenuPrimitive.Content>
      </DropdownMenuPrimitive.Portal>
    );
  }
);

DropdownContent.displayName = "DropdownContent";

// ---------------------------------------------------------------------------
// DropdownLabel
// ---------------------------------------------------------------------------

const DropdownLabel = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement>>(
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

DropdownLabel.displayName = "DropdownLabel";

// ---------------------------------------------------------------------------
// DropdownSeparator
// ---------------------------------------------------------------------------

const DropdownSeparator = forwardRef<
  HTMLDivElement,
  HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    role="separator"
    className={cn("my-1 -mx-1 h-px shrink-0 bg-border/60", className)}
    {...props}
  />
));

DropdownSeparator.displayName = "DropdownSeparator";

export {
  Dropdown,
  DropdownLabel,
  DropdownSeparator,
  DropdownMenu,
  DropdownTrigger,
  DropdownContent,
};
export type {
  DropdownProps,
  DropdownMenuProps,
  DropdownTriggerProps,
  DropdownContentProps,
};
export default Dropdown;
