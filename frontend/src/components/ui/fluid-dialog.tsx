"use client";

import {
  createContext,
  forwardRef,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ComponentPropsWithoutRef,
  type HTMLAttributes,
} from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { motion } from "framer-motion";
import { X } from "@/components/deslop-icons";
import { Button as FluidButton } from "@/components/ui/fluid-button";
import { cn } from "@/lib/utils";
import { useDialogEscapeGuard } from "@/lib/dialog-escape";
import { spring, exitFallbackMs } from "@/lib/springs";
import { useShape } from "@/lib/shape-context";
import { SurfaceProvider, useSurface } from "@/lib/surface-context";
import { surfaceClasses } from "@/lib/surface-classes";

const DIALOG_OFFSET = 4;
const DialogOpenContext = createContext(false);
const DialogCloseContext = createContext<(() => void) | null>(null);

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
  const close = useCallback(() => handleOpenChange(false), [onOpenChange]);

  return (
    <DialogOpenContext.Provider value={open}>
      <DialogCloseContext.Provider value={close}>
        <DialogPrimitive.Root open={open} onOpenChange={handleOpenChange} {...props}>
          {children}
        </DialogPrimitive.Root>
      </DialogCloseContext.Provider>
    </DialogOpenContext.Provider>
  );
}

const DialogTrigger = DialogPrimitive.Trigger;
const DialogClose = DialogPrimitive.Close;

interface DialogContentProps
  extends ComponentPropsWithoutRef<typeof DialogPrimitive.Content> {
  size?: "sm" | "lg";
  container?: HTMLElement | null;
}

const DialogContent = forwardRef<HTMLDivElement, DialogContentProps>(
  ({ className, children, size = "sm", container, ...props }, ref) => {
    const open = useContext(DialogOpenContext);
    const close = useContext(DialogCloseContext);
    const shape = useShape();
    const substrate = useSurface();
    const dialogLevel = Math.min(substrate + DIALOG_OFFSET, 8);
    const [mounted, setMounted] = useState(false);
    const popupRef = useDialogEscapeGuard({
      open,
      close,
      onEscapeKeyDown: props.onEscapeKeyDown,
    });

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
              "inset-0 z-[1000] bg-black/40 dark:bg-black/80",
            )}
            initial={{ opacity: 0 }}
            animate={{ opacity: open ? 1 : 0 }}
            transition={open ? spring.slow : spring.slow.exit}
          />
        </DialogPrimitive.Overlay>

        <DialogPrimitive.Content ref={ref} asChild forceMount {...props}>
          <motion.div
            ref={popupRef}
            className={cn(
              container ? "absolute" : "fixed",
              "left-1/2 top-1/2 z-[1001] w-[calc(100%-2rem)] p-6 focus:outline-none",
              surfaceClasses(dialogLevel),
              size === "sm" && "max-w-[400px]",
              size === "lg" && "max-w-[540px]",
              shape.container,
              className,
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
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className="absolute right-3 top-3"
                  aria-label="Закрыть"
                >
                  <X size={16} />
                </FluidButton>
              </DialogPrimitive.Close>
            </SurfaceProvider>
          </motion.div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    );
  },
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
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title
    ref={ref}
    className={cn("text-[16px] leading-tight text-foreground", className)}
    style={{ fontVariationSettings: "'wght' 700" }}
    {...props}
  />
));
DialogTitle.displayName = "DialogTitle";

const DialogDescription = forwardRef<
  HTMLParagraphElement,
  ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description
    ref={ref}
    className={cn("text-[13px] text-muted-foreground", className)}
    {...props}
  />
));
DialogDescription.displayName = "DialogDescription";

export {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
  DialogClose,
};
