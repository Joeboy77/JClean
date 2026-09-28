import { Folder } from "lucide-react";
import { ICONS } from "./ruleIcons";

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
