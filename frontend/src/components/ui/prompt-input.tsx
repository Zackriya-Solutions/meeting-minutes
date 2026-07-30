"use client"

import * as React from "react"
import { Loader2 } from "@/components/deslop-icons"
import { Icon } from "@/components/memento/Icon"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupTextarea,
} from "@/components/ui/input-group"
import { cn } from "@/lib/utils"

type PromptInputProps = Omit<
  React.ComponentProps<"textarea">,
  "onChange" | "value"
> & {
  value: string
  onValueChange: (value: string) => void
  onSubmit?: () => void
  inputRef?: React.Ref<HTMLTextAreaElement>
  sending?: boolean
  containerClassName?: string
  sendLabel: string
  submitButtonType?: "button" | "submit"
}

export function PromptInputContainer({
  className,
  ...props
}: React.ComponentProps<typeof InputGroup>) {
  return (
    <InputGroup
      className={cn(
        "min-h-[50px] items-center rounded-[24px] border-[var(--primary-10)] bg-[var(--elevation-1)] py-0 pl-4 pr-2 shadow-none dark:bg-[var(--elevation-1)]",
        "focus-within:border-[var(--primary-10)] has-[[data-slot=input-group-control]:focus-visible]:ring-0",
        className,
      )}
      {...props}
    />
  )
}

function assignRef<T>(ref: React.Ref<T> | undefined, value: T | null) {
  if (typeof ref === "function") ref(value)
  else if (ref) (ref as React.MutableRefObject<T | null>).current = value
}

export function PromptInput({
  value,
  onValueChange,
  onSubmit,
  inputRef,
  sending = false,
  disabled = false,
  containerClassName,
  className,
  sendLabel,
  submitButtonType = "button",
  onKeyDown,
  rows = 1,
  ...props
}: PromptInputProps) {
  const controlRef = React.useRef<HTMLTextAreaElement | null>(null)
  const canSend = !disabled && !sending && value.trim().length > 0

  const resize = React.useCallback((node: HTMLTextAreaElement | null) => {
    if (!node) return
    node.style.height = "auto"
    node.style.height = `${Math.min(node.scrollHeight, 240)}px`
  }, [])

  const setControlRef = React.useCallback(
    (node: HTMLTextAreaElement | null) => {
      controlRef.current = node
      assignRef(inputRef, node)
    },
    [inputRef],
  )

  React.useLayoutEffect(() => {
    resize(controlRef.current)
  }, [resize, value])

  return (
    <PromptInputContainer className={containerClassName}>
      <InputGroupTextarea
        ref={setControlRef}
        value={value}
        disabled={disabled}
        rows={rows}
        onChange={(event) => {
          onValueChange(event.target.value)
          resize(event.currentTarget)
        }}
        onKeyDown={(event) => {
          onKeyDown?.(event)
          if (
            event.defaultPrevented ||
            event.key !== "Enter" ||
            event.shiftKey ||
            event.nativeEvent.isComposing
          ) return

          event.preventDefault()
          if (!canSend) return
          if (submitButtonType === "submit") event.currentTarget.form?.requestSubmit()
          else onSubmit?.()
        }}
        className={cn(
          "h-auto min-h-[48px] max-h-[240px] resize-none overflow-y-auto pb-[10px] pl-0 pr-1 pt-[14px] text-sm leading-6",
          className,
        )}
        {...props}
      />
      <InputGroupAddon
        align="inline-end"
        className="self-center p-0 has-[>button]:mr-0"
      >
        <InputGroupButton
          type={submitButtonType}
          variant="ghost"
          size="icon-sm"
          disabled={!canSend}
          onClick={submitButtonType === "button" ? onSubmit : undefined}
          aria-label={sendLabel}
          className="h-9 w-9 rounded-full bg-[var(--primary-5)] text-muted-foreground hover:bg-[var(--primary-10)] hover:text-foreground"
        >
          {sending ? <Loader2 className="h-4 w-4 animate-spin" /> : <Icon name="send" size={17} />}
        </InputGroupButton>
      </InputGroupAddon>
    </PromptInputContainer>
  )
}
