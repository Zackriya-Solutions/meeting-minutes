"use client";

import {
  createContext,
  forwardRef,
  useContext,
  useEffect,
  useState,
  type ComponentPropsWithoutRef,
  type HTMLAttributes,
} from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { motion } from "framer-motion";

import { Button as FluidButton } from "@/components/ui/fluid-button";
import { useIcon } from "@/lib/icon-context";
import { cn } from "@/lib/utils";
import { spring, exitFallbackMs } from "@/lib/springs";
import { useShape } from "@/lib/shape-context";
import { useSize, useSizeVariant } from "@/lib/size-context";
import { SurfaceProvider, useSurface } from "@/lib/surface-context";
import { surfaceClasses } from "@/lib/surface-classes";

const DIALOG_OFFSET = 4;
const DIALOG_OVERLAY_Z = "z-[2000]";
const DIALOG_CONTENT_Z = "z-[2001]";

const DialogOpenContext = createContext(false);

function Dialog({
  children,
  open: controlledOpen,
  defaultOpen,
  onOpenChange,
  ...props
}: DialogPrimitive.DialogProps) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(defaultOpen ?? false);
  const open = controlledOpen ?? uncontrolledOpen;

  const handleOpenChange = (next: boolean) => {
    setUncontrolledOpen(next);
    onOpenChange?.(next);
  };

  return (
    <DialogOpenContext.Provider value={open}>
      <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange} {...props}>
        {children}
      </DialogPrimitive.Root>
    </DialogOpenContext.Provider>
  );
}

const DialogTrigger = DialogPrimitive.Trigger;
const DialogPortal = DialogPrimitive.Portal;
const DialogClose = DialogPrimitive.Close;

const DialogOverlay = forwardRef<
  React.ElementRef<typeof DialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay
    ref={ref}
    className={cn(
      "fixed inset-0 bg-black/40 dark:bg-black/80",
      DIALOG_OVERLAY_Z,
      className
    )}
    {...props}
  />
));
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName;

interface DialogContentProps
  extends ComponentPropsWithoutRef<typeof DialogPrimitive.Content> {
  size?: "sm" | "lg";
  container?: HTMLElement | null;
  overlayClassName?: string;
}

const DialogContent = forwardRef<HTMLDivElement, DialogContentProps>(
  (
    {
      className,
      children,
      size = "sm",
      container,
      overlayClassName,
      ...props
    },
    ref
  ) => {
    const XIcon = useIcon("x");
    const open = useContext(DialogOpenContext);
    const shape = useShape();
    const substrate = useSurface();
    const dialogLevel = Math.min(substrate + DIALOG_OFFSET, 8);
    const compact = useSize().variant === "compact";
    const [mounted, setMounted] = useState(false);

    useEffect(() => {
      if (open) setMounted(true);
    }, [open]);

    useEffect(() => {
      if (open) return;
      const id = setTimeout(() => setMounted(false), exitFallbackMs(spring.slow));
      return () => clearTimeout(id);
    }, [open]);

    if (!mounted) return null;

    return (
      <DialogPrimitive.Portal forceMount container={container ?? undefined}>
        <DialogPrimitive.Overlay asChild forceMount>
          <motion.div
            className={cn(
              container ? "absolute" : "fixed",
              "inset-0 bg-black/40 dark:bg-black/80",
              DIALOG_OVERLAY_Z,
              overlayClassName
            )}
            initial={{ opacity: 0 }}
            animate={{ opacity: open ? 1 : 0 }}
            transition={open ? spring.slow : spring.slow.exit}
          />
        </DialogPrimitive.Overlay>

        <DialogPrimitive.Content ref={ref} asChild forceMount {...props}>
          <motion.div
            className={cn(
              container ? "absolute" : "fixed",
              "left-1/2 top-1/2 w-[calc(100%-2rem)]",
              DIALOG_CONTENT_Z,
              surfaceClasses(dialogLevel),
              "p-6 focus:outline-none",
              size === "sm" && (compact ? "max-w-[360px]" : "max-w-[400px]"),
              size === "lg" && (compact ? "max-w-[480px]" : "max-w-[540px]"),
              shape.container,
              className
            )}
            initial={{ opacity: 0, scale: 0.97, x: "-50%", y: "-50%" }}
            animate={{
              opacity: open ? 1 : 0,
              scale: open ? 1 : 0.97,
              x: "-50%",
              y: "-50%",
            }}
            transition={open ? spring.slow : spring.slow.exit}
            onAnimationComplete={() => {
              if (!open) setMounted(false);
            }}
          >
            <SurfaceProvider value={dialogLevel}>
              {children}
              <DialogPrimitive.Close asChild>
                <FluidButton
                  variant="ghost"
                  size="icon-sm"
                  className="absolute right-3 top-3"
                >
                  <XIcon />
                  <span className="sr-only">Close</span>
                </FluidButton>
              </DialogPrimitive.Close>
            </SurfaceProvider>
          </motion.div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    );
  }
);
DialogContent.displayName = "DialogContent";

function DialogHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("mb-4 flex flex-col gap-1.5", className)} {...props} />;
}

function DialogFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("mt-6 flex justify-end gap-2", className)} {...props} />;
}

const DialogTitle = forwardRef<
  HTMLHeadingElement,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, style, ...props }, ref) => {
  const compact = useSizeVariant() === "compact";
  return (
    <DialogPrimitive.Title
      ref={ref}
      className={cn(
        compact ? "text-[15px]" : "text-[16px]",
        "leading-tight text-foreground",
        className
      )}
      style={{ fontVariationSettings: "'wght' 700", ...style }}
      {...props}
    />
  );
});
DialogTitle.displayName = "DialogTitle";

const DialogDescription = forwardRef<
  HTMLParagraphElement,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => {
  const compact = useSizeVariant() === "compact";
  return (
    <DialogPrimitive.Description
      ref={ref}
      className={cn(
        compact ? "text-[12px]" : "text-[13px]",
        "text-muted-foreground",
        className
      )}
      {...props}
    />
  );
});
DialogDescription.displayName = "DialogDescription";

export {
  Dialog,
  DialogPortal,
  DialogOverlay,
  DialogTrigger,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
};
