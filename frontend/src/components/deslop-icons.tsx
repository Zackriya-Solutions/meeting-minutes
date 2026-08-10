import { forwardRef, type ForwardRefExoticComponent, type HTMLAttributes, type RefAttributes } from 'react';
import { MaterialSymbol } from '@/vendor/deslop/material-symbols-react';
import type { MaterialSymbolName } from '@/vendor/deslop/material-symbols-react';

export interface DeslopIconProps extends Omit<HTMLAttributes<HTMLSpanElement>, 'children' | 'color'> {
  size?: number | string;
  color?: string;
  weight?: number | string;
  strokeWidth?: number | string;
  absoluteStrokeWidth?: boolean;
  fill?: boolean | number | string;
  width?: number | string;
  height?: number | string;
}

// Compatibility types let existing component contracts migrate without
// retaining a runtime or type dependency on Lucide.
export type LucideProps = DeslopIconProps;
export type LucideIcon = ForwardRefExoticComponent<DeslopIconProps & RefAttributes<HTMLSpanElement>>;

function createIcon(symbol: MaterialSymbolName, displayName: string): LucideIcon {
  const Component = forwardRef<HTMLSpanElement, DeslopIconProps>(function DeslopIcon(
    {
      size,
      color,
      weight: requestedWeight,
      strokeWidth,
      absoluteStrokeWidth: _absoluteStrokeWidth,
      fill,
      width: _width,
      height: _height,
      style,
      ...props
    },
    ref,
  ) {
    const numericStroke = Number(strokeWidth);
    const numericWeight = Number(requestedWeight);
    const weight = Number.isFinite(numericWeight)
      ? Math.min(700, Math.max(100, numericWeight))
      : Number.isFinite(numericStroke)
        ? Math.min(700, Math.max(100, Math.round(100 + numericStroke * 150)))
        : 400;
    const filled = fill === true || fill === 1;

    return (
      <MaterialSymbol
        {...props}
        ref={ref}
        name={symbol}
        size={size ?? '1em'}
        fill={filled}
        weight={weight}
        style={{ color, ...style }}
      />
    );
  });
  Component.displayName = displayName;
  return Component;
}

const SpinnerIcon = forwardRef<HTMLSpanElement, DeslopIconProps>(function SpinnerIcon(
  {
    size = '1em',
    color,
    strokeWidth: _strokeWidth,
    absoluteStrokeWidth: _absoluteStrokeWidth,
    fill: _fill,
    width: _width,
    height: _height,
    className,
    style,
    title,
    ...props
  },
  ref,
) {
  const labelled = Boolean(title || props['aria-label'] || props['aria-labelledby']);

  return (
    <span
      {...props}
      ref={ref}
      title={title}
      role={props.role ?? (labelled ? 'img' : undefined)}
      aria-hidden={props['aria-hidden'] ?? (labelled ? undefined : true)}
      className={['memento-spinner', className].filter(Boolean).join(' ')}
      style={{
        color,
        fontSize: typeof size === 'number' ? `${size}px` : size,
        ...style,
      }}
    />
  );
});
SpinnerIcon.displayName = 'SpinnerIcon';

export const AlertCircle = createIcon('error', 'AlertCircle');
export const AlertTriangle = createIcon('warning', 'AlertTriangle');
export const ArrowBigDownDash = createIcon('download', 'ArrowBigDownDash');
export const ArrowLeft = createIcon('arrow_back', 'ArrowLeft');
export const AudioWaveform = createIcon('bar_chart', 'AudioWaveform');
export const BadgeAlert = createIcon('warning', 'BadgeAlert');
export const BrainCircuit = createIcon('analytics', 'BrainCircuit');
export const Calendar = createIcon('calendar_month', 'Calendar');
export const CalendarDays = Calendar;
export const Checklist = createIcon('checklist' as MaterialSymbolName, 'Checklist');
export const Check = createIcon('check', 'Check');
export const CheckCircle = createIcon('check_circle', 'CheckCircle');
export const CheckCircle2 = CheckCircle;
export const ChevronDown = createIcon('expand_more', 'ChevronDown');
export const ChevronRight = createIcon('chevron_right', 'ChevronRight');
export const ChevronUp = createIcon('expand_less', 'ChevronUp');
export const ChevronsUpDown = createIcon('unfold_more', 'ChevronsUpDown');
export const Circle = createIcon('radio_button_unchecked', 'Circle');
export const CircleCheck = CheckCircle;
export const CircleX = createIcon('cancel', 'CircleX');
export const Clock = createIcon('schedule', 'Clock');
export const Clock3 = Clock;
export const Copy = createIcon('content_copy', 'Copy');
export const Cpu = createIcon('terminal', 'Cpu');
export const Database = createIcon('database', 'Database');
export const Download = createIcon('download', 'Download');
export const Event = createIcon('event', 'Event');
export const ExternalLink = createIcon('open_in_new', 'ExternalLink');
export const Eye = createIcon('visibility', 'Eye');
export const EyeOff = createIcon('visibility_off', 'EyeOff');
export const FileAudio = createIcon('attach_file', 'FileAudio');
export const FileQuestion = createIcon('help', 'FileQuestion');
export const FileText = createIcon('description', 'FileText');
export const FlaskConical = createIcon('bolt', 'FlaskConical');
export const Folder = createIcon('folder', 'Folder');
export const FolderOpen = createIcon('folder_open', 'FolderOpen');
export const Globe = createIcon('language', 'Globe');
export const GlobeIcon = Globe;
export const HardDrive = createIcon('database', 'HardDrive');
export const House = createIcon('home', 'House');
export const Info = createIcon('info', 'Info');
export const KeyRound = createIcon('lock', 'KeyRound');
export const Languages = createIcon('language', 'Languages');
export const Layers3 = createIcon('apps', 'Layers3');
export const List = createIcon('menu', 'List');
export const Loader2 = SpinnerIcon;
export const LoaderCircle = Loader2;
export const LoaderIcon = Loader2;
export const Lock = createIcon('lock', 'Lock');
export const MessageSquare = createIcon('chat', 'MessageSquare');
export const Mic = createIcon('mic', 'Mic');
export const Minus = createIcon('remove', 'Minus');
export const MoreHorizontal = createIcon('more_horiz', 'MoreHorizontal');
export const Pause = createIcon('remove_circle', 'Pause');
export const Pencil = createIcon('edit', 'Pencil');
export const Pin = createIcon('flag', 'Pin');
export const Play = createIcon('play_arrow', 'Play');
export const Plus = createIcon('add', 'Plus');
export const RefreshCw = createIcon('refresh', 'RefreshCw');
export const RotateCw = RefreshCw;
export const Save = createIcon('save', 'Save');
export const Search = createIcon('search', 'Search');
export const Send = createIcon('send', 'Send');
export const Settings = createIcon('settings', 'Settings');
export const Shield = createIcon('shield', 'Shield');
export const SlidersHorizontal = createIcon('tune', 'SlidersHorizontal');
export const Sparkles = createIcon('workspace_premium', 'Sparkles');
export const Speaker = createIcon('notifications', 'Speaker');
export const Square = createIcon('cancel', 'Square');
export const Tag = createIcon('badge', 'Tag');
export const Trash2 = createIcon('delete', 'Trash2');
export const TriangleAlert = AlertTriangle;
export const Unlock = createIcon('lock', 'Unlock');
export const Upload = createIcon('upload', 'Upload');
export const Users = createIcon('person', 'Users');
export const Volume2 = Speaker;
export const X = createIcon('close', 'X');
export const XCircle = CircleX;
