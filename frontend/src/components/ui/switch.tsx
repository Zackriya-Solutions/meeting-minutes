"use client";

import {
  forwardRef,
  useRef,
  useState,
  useEffect,
  useCallback,
  useId,
  type HTMLAttributes,
} from "react";
import { motion, useMotionValue, animate, type Transition } from "framer-motion";
import { Switch as SwitchPrimitive } from "@base-ui/react/switch";
import { cn } from "@/lib/utils";
import { spring } from "@/lib/fluid/springs";

interface SwitchProps extends Omit<HTMLAttributes<HTMLDivElement>, "onChange" | "onToggle"> {
  label?: string;
  checked: boolean;
  onToggle?: () => void;
  onCheckedChange?: (checked: boolean) => void;
  disabled?: boolean;
  thumbTransition?: Transition;
}

const TRACK_WIDTH = 34;
const TRACK_HEIGHT = 20;
const THUMB_SIZE = 16;
const THUMB_OFFSET = 2;
const THUMB_TRAVEL = TRACK_WIDTH - THUMB_SIZE - THUMB_OFFSET * 2;
const PILL_EXTEND = 2;
const PRESS_EXTEND = 4;
const PRESS_SHRINK = 4;
const DRAG_DEAD_ZONE = 2;

const Switch = forwardRef<HTMLDivElement, SwitchProps>(
  ({
    label,
    checked,
    onToggle,
    onCheckedChange,
    disabled = false,
    thumbTransition,
    className,
    ...props
  }, ref) => {
    const labelId = useId();
    const hasMounted = useRef(false);
    const [hovered, setHovered] = useState(false);
    const [pressed, setPressed] = useState(false);

    const dragging = useRef(false);
    const didDrag = useRef(false);
    const pointerStart = useRef<{
      clientX: number;
      originX: number;
    } | null>(null);

    const motionX = useMotionValue(
      checked ? THUMB_OFFSET + THUMB_TRAVEL : THUMB_OFFSET
    );

    useEffect(() => {
      hasMounted.current = true;
    }, []);

    const thumbWidth = pressed
      ? THUMB_SIZE + PRESS_EXTEND
      : hovered
        ? THUMB_SIZE + PILL_EXTEND
        : THUMB_SIZE;
    const thumbHeight = pressed ? THUMB_SIZE - PRESS_SHRINK : THUMB_SIZE;
    const thumbY = pressed ? THUMB_OFFSET + PRESS_SHRINK / 2 : THUMB_OFFSET;
    const extraWidth = thumbWidth - THUMB_SIZE;
    const thumbX = checked
      ? THUMB_OFFSET + THUMB_TRAVEL - extraWidth
      : THUMB_OFFSET;

    const commitToggle = useCallback(() => {
      if (disabled) return;
      onToggle?.();
      onCheckedChange?.(!checked);
    }, [checked, disabled, onCheckedChange, onToggle]);

    const animateThumbTo = useCallback((target: number) => {
      animate(motionX.get(), target, {
        ...(thumbTransition ?? spring.moderate),
        onUpdate: (value: number) => motionX.set(value),
      } as any);
    }, [motionX, thumbTransition]);

    useEffect(() => {
      if (dragging.current) return;
      if (!hasMounted.current) {
        motionX.set(thumbX);
      } else {
        animateThumbTo(thumbX);
      }
    }, [animateThumbTo, thumbX, motionX]);

    const handlePointerDown = useCallback(
      (e: React.PointerEvent<HTMLDivElement>) => {
        if (disabled) return;
        if (e.pointerType === "mouse" && e.button !== 0) return;
        setPressed(true);
        dragging.current = false;
        didDrag.current = false;
        pointerStart.current = {
          clientX: e.clientX,
          originX: motionX.get(),
        };
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      },
      [disabled, motionX]
    );

    const handlePointerMove = useCallback(
      (e: React.PointerEvent<HTMLDivElement>) => {
        if (!pointerStart.current) return;
        const delta = e.clientX - pointerStart.current.clientX;

        if (!dragging.current) {
          if (Math.abs(delta) < DRAG_DEAD_ZONE) return;
          dragging.current = true;
        }

        const dragMin = THUMB_OFFSET;
        const pressedThumbWidth = THUMB_SIZE + PRESS_EXTEND;
        const dragMax = TRACK_WIDTH - THUMB_OFFSET - pressedThumbWidth;
        const rawX = pointerStart.current.originX + delta;
        motionX.set(Math.max(dragMin, Math.min(dragMax, rawX)));
      },
      [motionX]
    );

    const handlePointerUp = useCallback(
      () => {
        if (!pointerStart.current) return;
        setPressed(false);

        if (dragging.current) {
          didDrag.current = true;
          dragging.current = false;

          const currentX = motionX.get();
          const dragMin = THUMB_OFFSET;
          const pressedThumbWidth = THUMB_SIZE + PRESS_EXTEND;
          const dragMax = TRACK_WIDTH - THUMB_OFFSET - pressedThumbWidth;
          const midpoint = (dragMin + dragMax) / 2;

          const shouldBeOn = currentX > midpoint;

          if (shouldBeOn !== checked) {
            commitToggle();
          } else {
            const snapTarget = checked
              ? THUMB_OFFSET + THUMB_TRAVEL
              : THUMB_OFFSET;
            animateThumbTo(snapTarget);
          }

          requestAnimationFrame(() => {
            didDrag.current = false;
          });
        }

        pointerStart.current = null;
      },
      [animateThumbTo, checked, commitToggle, motionX]
    );

    const handlePointerCancel = useCallback(
      () => {
        if (!pointerStart.current) return;
        setPressed(false);

        if (dragging.current) {
          dragging.current = false;
          const snapTarget = checked
            ? THUMB_OFFSET + THUMB_TRAVEL
            : THUMB_OFFSET;
          animateThumbTo(snapTarget);
        }

        pointerStart.current = null;
      },
      [animateThumbTo, checked]
    );

    return (
      <div
        ref={ref}
        className={cn(
          "relative z-10 inline-flex cursor-pointer select-none touch-none items-center",
          label && "gap-2.5 px-3 py-2",
          disabled && "opacity-50 pointer-events-none",
          className
        )}
        onPointerEnter={(e) => {
          if (e.pointerType === "mouse") setHovered(true);
        }}
        onPointerLeave={() => setHovered(false)}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerCancel}
        onClick={() => {
          if (disabled || didDrag.current) return;
          commitToggle();
        }}
        {...props}
      >
        {/* Switch */}
        <SwitchPrimitive.Root
          checked={checked}
          aria-labelledby={label ? labelId : undefined}
          // Base UI passes (checked, eventDetails); narrow to () => void for our onToggle.
          onCheckedChange={() => {
            if (didDrag.current) return;
            commitToggle();
          }}
          disabled={disabled}
          tabIndex={0}
          className={cn(
            "relative shrink-0 rounded-full outline-none cursor-pointer",
            "transition-colors duration-80",
            "focus-visible:ring-1 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-background"
          )}
          style={{
            width: TRACK_WIDTH,
            height: TRACK_HEIGHT,
            backgroundColor: checked
              ? hovered
                ? "color-mix(in srgb, var(--accent-orange) 88%, var(--white))"
                : "var(--accent-orange)"
              : hovered
                ? "var(--deslop-primary-20)"
                : "var(--primary-10)",
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <SwitchPrimitive.Thumb
            render={(props) => {
              const {
                style: baseStyle,
                onDrag: _onDrag,
                onDragStart: _onDragStart,
                onDragEnd: _onDragEnd,
                onAnimationStart: _onAnimationStart,
                onAnimationEnd: _onAnimationEnd,
                onAnimationIteration: _onAnimationIteration,
                ...rest
              } = props as React.HTMLAttributes<HTMLSpanElement>;
              return (
                <motion.span
                  {...rest}
                  className="absolute left-0 top-0 block rounded-full bg-[var(--white)] shadow-sm"
                  initial={false}
                  style={{
                    ...(baseStyle as React.CSSProperties | undefined),
                    x: motionX,
                  }}
                  animate={{
                    y: thumbY,
                    width: thumbWidth,
                    height: thumbHeight,
                  }}
                  transition={hasMounted.current ? (thumbTransition ?? spring.moderate) : { duration: 0 }}
                />
              );
            }}
          />
        </SwitchPrimitive.Root>

        {/* Label */}
        {label && (
          <span
            id={labelId}
            className={cn(
              // text-box trim recenters the letterforms against the track; the
              // 20px track is taller than the label, so layout doesn't change.
              "text-[13px] [text-box:trim-both_cap_alphabetic] transition-[color] duration-80",
              checked ? "text-foreground" : "text-muted-foreground"
            )}
          >
            {label}
          </span>
        )}
      </div>
    );
  }
);

Switch.displayName = "Switch";

export { Switch };
export type { SwitchProps };
