import { FluidSpinner } from "@/components/ui/fluid-spinner";

export function MeetingDrawerLoading() {
  return (
    <div className="flex h-full items-center justify-center">
      <FluidSpinner className="size-6 text-[var(--primary-50)]" />
    </div>
  );
}

export default MeetingDrawerLoading;
