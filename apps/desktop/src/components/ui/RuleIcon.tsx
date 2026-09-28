import {
  AppWindow,
  Archive,
  Beer,
  Brain,
  Code,
  Compass,
  Container,
  Database,
  FileBox,
  FileWarning,
  Folder,
  FolderCog,
  FolderOutput,
  FolderTree,
  Globe,
  Hammer,
  HardDrive,
  Hexagon,
  History,
  Images,
  Layers,
  Mail,
  MessageCircle,
  MessageSquare,
  Music,
  Package,
  PackageOpen,
  Puzzle,
  ScrollText,
  Smartphone,
  Trash2,
  Video,
  type LucideIcon,
} from "lucide-react";

// Rule files name icons in kebab case; only these are bundled.
const ICONS: Record<string, LucideIcon> = {
  "app-window": AppWindow,
  archive: Archive,
  beer: Beer,
  brain: Brain,
  code: Code,
  compass: Compass,
  container: Container,
  database: Database,
  "file-box": FileBox,
  "file-warning": FileWarning,
  "folder-cog": FolderCog,
  "folder-output": FolderOutput,
  "folder-tree": FolderTree,
  globe: Globe,
  hammer: Hammer,
  "hard-drive": HardDrive,
  hexagon: Hexagon,
  history: History,
  images: Images,
  layers: Layers,
  mail: Mail,
  "message-circle": MessageCircle,
  "message-square": MessageSquare,
  music: Music,
  package: Package,
  "package-open": PackageOpen,
  puzzle: Puzzle,
  "scroll-text": ScrollText,
  smartphone: Smartphone,
  trash: Trash2,
  video: Video,
};

interface RuleIconProps {
  name: string;
  size?: number;
  className?: string;
}

export function RuleIcon({ name, size = 16, className }: RuleIconProps) {
  const Icon = ICONS[name] ?? Folder;
  return (
    <Icon size={size} strokeWidth={1.75} aria-hidden="true" {...(className ? { className } : {})} />
  );
}
