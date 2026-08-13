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
import * as AlertDialogPrimitive from "@radix-ui/react-alert-dialog";
import { motion } from "framer-motion";

import { Button as FluidButton } from "@/components/ui/fluid-button";
import { cn } from "@/lib/utils";
import { spring, exitFallbackMs } from "@/lib/springs";
import { useShape } from "@/lib/shape-context";
import { SurfaceProvider, useSurface } from "@/lib/surface-context";
import { surfaceClasses } from "@/lib/surface-classes";

const DIALOG_OFFSET = 4;
const AlertDialogOpenContext = createContext(false);

function AlertDialog({
  children,
  open: controlledOpen,
  defaultOpen,
  onOpenChange,
  ...props
}: React.ComponentProps<typeof AlertDialogPrimitive.Root>) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(defaultOpen ?? false);
  const open = controlledOpen ?? uncontrolledOpen;

  const handleOpenChange = (next: boolean) => {
    setUncontrolledOpen(next);
    onOpenChange?.(next);
  };

  return (
    <AlertDialogOpenContext.Provider value={open}>
      <AlertDialogPrimitive.Root open={open} onOpenChange={handleOpenChange} {...props}>
        {children}
      </AlertDialogPrimitive.Root>
    </AlertDialogOpenContext.Provider>
  );
}

const AlertDialogTrigger = AlertDialogPrimitive.Trigger;
const AlertDialogPortal = AlertDialogPrimitive.Portal;

const AlertDialogOverlay = forwardRef<
  React.ElementRef<typeof AlertDialogPrimitive.Overlay>,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Overlay
    ref={ref}
    className={cn("fixed inset-0 z-[2000] bg-black/40 dark:bg-black/80", className)}
    {...props}
  />
));
AlertDialogOverlay.displayName = AlertDialogPrimitive.Overlay.displayName;

const AlertDialogContent = forwardRef<
  HTMLDivElement,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Content>
>(({ className, children, ...props }, ref) => {
  const open = useContext(AlertDialogOpenContext);
  const shape = useShape();
  const substrate = useSurface();
  const dialogLevel = Math.min(substrate + DIALOG_OFFSET, 8);
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
    <AlertDialogPrimitive.Portal forceMount>
      <AlertDialogPrimitive.Overlay asChild forceMount>
        <motion.div
          className="fixed inset-0 z-[2000] bg-black/40 dark:bg-black/80"
          initial={{ opacity: 0 }}
          animate={{ opacity: open ? 1 : 0 }}
          transition={open ? spring.slow : spring.slow.exit}
        />
      </AlertDialogPrimitive.Overlay>
      <AlertDialogPrimitive.Content ref={ref} asChild forceMount {...props}>
        <motion.div
          className={cn(
            "fixed left-1/2 top-1/2 z-[2001] grid w-[calc(100%-2rem)] max-w-[400px] gap-4 p-6 focus:outline-none",
            surfaceClasses(dialogLevel),
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
          <SurfaceProvider value={dialogLevel}>{children}</SurfaceProvider>
        </motion.div>
      </AlertDialogPrimitive.Content>
    </AlertDialogPrimitive.Portal>
  );
});
AlertDialogContent.displayName = "AlertDialogContent";

function AlertDialogHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-col gap-1.5", className)} {...props} />;
}

function AlertDialogFooter({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("mt-2 flex justify-end gap-2", className)} {...props} />;
}

const AlertDialogTitle = forwardRef<
  HTMLHeadingElement,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Title>
>(({ className, style, ...props }, ref) => (
  <AlertDialogPrimitive.Title
    ref={ref}
    className={cn("text-[16px] leading-tight text-foreground", className)}
    style={{ fontVariationSettings: "'wght' 700", ...style }}
    {...props}
  />
));
AlertDialogTitle.displayName = AlertDialogPrimitive.Title.displayName;

const AlertDialogDescription = forwardRef<
  HTMLParagraphElement,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <AlertDialogPrimitive.Description
    ref={ref}
    className={cn("text-[13px] text-muted-foreground", className)}
    {...props}
  />
));
AlertDialogDescription.displayName = AlertDialogPrimitive.Description.displayName;

const AlertDialogAction = forwardRef<
  HTMLButtonElement,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Action>
>(({ className, children, ...props }, ref) => (
  <AlertDialogPrimitive.Action asChild {...props}>
    <FluidButton ref={ref} variant="destructive" className={className}>
      {children}
    </FluidButton>
  </AlertDialogPrimitive.Action>
));
AlertDialogAction.displayName = AlertDialogPrimitive.Action.displayName;

const AlertDialogCancel = forwardRef<
  HTMLButtonElement,
  ComponentPropsWithoutRef<typeof AlertDialogPrimitive.Cancel>
>(({ className, children, ...props }, ref) => (
  <AlertDialogPrimitive.Cancel asChild {...props}>
    <FluidButton ref={ref} variant="secondary" className={className}>
      {children}
    </FluidButton>
  </AlertDialogPrimitive.Cancel>
));
AlertDialogCancel.displayName = AlertDialogPrimitive.Cancel.displayName;

export {
  AlertDialog,
  AlertDialogPortal,
  AlertDialogOverlay,
  AlertDialogTrigger,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogFooter,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogAction,
  AlertDialogCancel,
};
