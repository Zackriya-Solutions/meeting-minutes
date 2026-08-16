import { useEffect, useRef, type RefObject } from "react";

/**
 * Keep an Escape press inside the dialog that owns it.
 *
 * The app stacks two independent dismissal systems: dialogs are Radix, while the meeting,
 * recording and chat routes are Base UI drawers. Each listens for Escape on `document` and
 * neither knows the other exists, so a single press dismissed both layers — cancelling a
 * speaker rename also closed the meeting and navigated the user back to the meeting list.
 *
 * The guard listens on `document` in the capture phase, which runs before every bubble-phase
 * listener and before any capture listener registered on a deeper node — the one position
 * that does not depend on which library registered first, or in which phase. A press that
 * belongs to this dialog (its target is inside the popup) is resolved here: the dialog's own
 * `onEscapeKeyDown` runs, the dialog closes unless that handler prevented the default, and
 * `stopImmediatePropagation` keeps every other listener out of it.
 *
 * Stacked dialogs do not fight over this. Each popup is portaled to the body rather than
 * nested inside the one below, so a given press is inside exactly one of them.
 */
export function useDialogEscapeGuard({
  open,
  close,
  onEscapeKeyDown,
}: {
  open: boolean;
  close: (() => void) | null;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
}): RefObject<HTMLDivElement> {
  const popupRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      const node = popupRef.current;
      // The popup mounts a frame after `open` flips; until then there is nothing to own
      // the key, and letting it through is the pre-existing behaviour.
      if (!node) return;
      const target = event.target;
      if (!(target instanceof Node) || !node.contains(target)) return;

      event.stopImmediatePropagation();
      onEscapeKeyDown?.(event);
      // A dialog that must not be dismissed mid-flight (an import, a migration) says so by
      // preventing the default in its own handler. Honour that and stay open — but still
      // keep the key from reaching the drawer underneath.
      if (event.defaultPrevented) return;
      close?.();
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => document.removeEventListener("keydown", handleKeyDown, true);
  }, [open, close, onEscapeKeyDown]);

  return popupRef;
}
