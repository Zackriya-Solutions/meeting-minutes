import type { LucideIcon, LucideProps } from '@/components/deslop-icons';
import {
  ArrowLeft,
  AudioWaveform,
  CalendarDays,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Circle,
  CircleCheck,
  CircleX,
  Clock3,
  Copy,
  Database,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  FileText,
  Folder,
  Globe,
  House,
  Info,
  Layers3,
  List,
  LoaderCircle,
  Lock,
  MessageSquare,
  Mic,
  Minus,
  Pause,
  Pencil,
  Pin,
  Play,
  Plus,
  RotateCw,
  Save,
  Search,
  Send,
  Settings,
  SlidersHorizontal,
  Sparkles,
  Square,
  Tag,
  Trash2,
  TriangleAlert,
  Unlock,
  Upload,
  Users,
  Volume2,
  X,
} from '@/components/deslop-icons';

export type MementoIconName =
  | 'wave' | 'mic' | 'stop' | 'play' | 'pause' | 'transcript' | 'library'
  | 'search' | 'spark' | 'tag' | 'clock' | 'calendar' | 'users' | 'plus'
  | 'minus' | 'check' | 'check-circle' | 'chevron-right' | 'chevron-down'
  | 'chevron-up' | 'back' | 'close' | 'close-circle' | 'settings' | 'chat'
  | 'upload' | 'download' | 'home' | 'filter' | 'send' | 'alert' | 'circle'
  | 'copy' | 'database' | 'external' | 'eye' | 'eye-off' | 'folder' | 'globe'
  | 'info' | 'loader' | 'lock' | 'unlock' | 'edit' | 'pin' | 'refresh'
  | 'save' | 'speaker' | 'trash' | 'dot';

const icons: Record<MementoIconName, LucideIcon> = {
  wave: AudioWaveform,
  mic: Mic,
  stop: Square,
  play: Play,
  pause: Pause,
  transcript: List,
  library: Layers3,
  search: Search,
  spark: Sparkles,
  tag: Tag,
  clock: Clock3,
  calendar: CalendarDays,
  users: Users,
  plus: Plus,
  minus: Minus,
  check: Check,
  'check-circle': CircleCheck,
  'chevron-right': ChevronRight,
  'chevron-down': ChevronDown,
  'chevron-up': ChevronUp,
  back: ArrowLeft,
  close: X,
  'close-circle': CircleX,
  settings: Settings,
  chat: MessageSquare,
  upload: Upload,
  download: Download,
  home: House,
  filter: SlidersHorizontal,
  send: Send,
  alert: TriangleAlert,
  circle: Circle,
  copy: Copy,
  database: Database,
  external: ExternalLink,
  eye: Eye,
  'eye-off': EyeOff,
  folder: Folder,
  globe: Globe,
  info: Info,
  loader: LoaderCircle,
  lock: Lock,
  unlock: Unlock,
  edit: Pencil,
  pin: Pin,
  refresh: RotateCw,
  save: Save,
  speaker: Volume2,
  trash: Trash2,
  dot: Circle,
};

export interface IconProps extends Omit<LucideProps, 'name'> {
  name: MementoIconName;
}

/**
 * Product-level semantic icon adapter. Glyphs come exclusively from Lucide,
 * which is the icon set used by shadcn/ui.
 */
export function Icon({ name, size = 20, strokeWidth = 1.9, ...props }: IconProps) {
  const Glyph = icons[name];
  return <Glyph aria-hidden="true" size={size} strokeWidth={strokeWidth} {...props} />;
}
