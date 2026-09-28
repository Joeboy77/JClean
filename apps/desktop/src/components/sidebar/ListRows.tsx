import { ChevronRight, Info } from "lucide-react";
import { motion } from "motion/react";
import type { Audience } from "../../data/types";
import { formatAgo, formatBytes, speakBytes, tildify } from "../../lib/format";
import { spring } from "../../lib/motion";
import {
  countLabel,
  itemName,
  ruleLabel,
  type GroupRow,
  type ItemRow,
  type SectionRow,
} from "../../state/selectors";
import { Checkbox } from "../ui/Checkbox";
import { RISK_COPY } from "../../data/riskCopy";
import { RiskIcon } from "../ui/RiskBadge";
import { RuleIcon } from "../ui/RuleIcon";
import { Tooltip } from "../ui/Tooltip";

function rowClass(active: boolean): string {
  // The keyboard cursor only shows while the list has keyboard focus.
  return `flex h-full items-center gap-3 rounded-row px-2 transition-colors hover:bg-raised/60 ${
    active
      ? "group-focus-visible/list:bg-raised group-focus-visible/list:ring-1 group-focus-visible/list:ring-accent"
      : ""
  }`;
}

function Chevron({ open }: { open: boolean }) {
  return (
    <motion.span
      animate={{ rotate: open ? 90 : 0 }}
      transition={spring}
      className="grid place-items-center"
    >
      <ChevronRight size={14} strokeWidth={2} aria-hidden="true" className="text-muted" />
    </motion.span>
  );
}

interface SectionProps {
  row: SectionRow;
  active: boolean;
  onToggle: () => void;
}

export function SectionHeader({ row, active, onToggle }: SectionProps) {
  const copy = RISK_COPY[row.risk];
  return (
    <div className={`${rowClass(active)} cursor-pointer`} onClick={onToggle}>
      <Chevron open={!row.collapsed} />
      <RiskIcon risk={row.risk} />
      <span className="font-medium text-text">
        {copy.title} <span className="tabular text-muted">({row.count})</span>
      </span>
      <span className="flex-1" />
      <span className="tabular text-muted">{formatBytes(row.bytes)}</span>
      <Tooltip text={copy.why} align="end">
        <span
          tabIndex={-1}
          aria-label={copy.why}
          className="grid place-items-center text-muted hover:text-text"
        >
          <Info size={14} aria-hidden="true" />
        </span>
      </Tooltip>
    </div>
  );
}

function secondaryForGroup(row: GroupRow, audience: Audience, home: string): string {
  const parts: string[] = [];
  const only = row.items.length === 1 ? row.items[0] : undefined;
  // The count comes first so it never gets cut off in narrow windows.
  if (!only) parts.push(countLabel(row.items));
  if (row.lastUsed !== null) parts.push(`Last used ${formatAgo(row.lastUsed)}`);
  if (only?.blockedReason) {
    parts.push(only.blockedReason);
  } else if (only?.project) {
    parts.push(only.project.active ? `${only.project.name} · active project` : only.project.name);
  } else if (only?.name) {
    parts.push(only.name);
  } else if (only && audience === "developer" && only.path) {
    parts.push(tildify(only.path, home));
  }
  return parts.join(" · ");
}

function sizeLabel(bytes: number, shared: boolean): string {
  return shared ? `up to ${formatBytes(bytes)}` : formatBytes(bytes);
}

interface GroupProps {
  row: GroupRow;
  active: boolean;
  audience: Audience;
  home: string;
  onToggleCheck: () => void;
  onToggleExpand: () => void;
  onOpen: () => void;
}

export function GroupRowView({
  row,
  active,
  audience,
  home,
  onToggleCheck,
  onToggleExpand,
  onOpen,
}: GroupProps) {
  const label = ruleLabel(row.rule, audience);
  const shared = row.items.some((i) => i.mayShareBlocks);
  return (
    <div className={rowClass(active)}>
      <Checkbox
        state={row.check}
        disabled={row.disabled}
        label={`${label}, ${speakBytes(row.bytes)}, ${RISK_COPY[row.risk].title.toLowerCase()}`}
        onToggle={onToggleCheck}
      />
      <span
        className="grid size-7 shrink-0 place-items-center rounded-row bg-raised"
        style={{ color: `var(--cat-${row.rule.category})` }}
      >
        <RuleIcon name={row.rule.icon} size={15} />
      </span>
      <span className="min-w-0 flex-1">
        <button
          type="button"
          tabIndex={-1}
          onClick={(e) => {
            e.stopPropagation();
            onOpen();
          }}
          className="block max-w-full truncate text-left text-text hover:underline"
        >
          {label}
        </button>
        <span className="block truncate text-xs text-muted">
          {secondaryForGroup(row, audience, home)}
        </span>
      </span>
      <span className={`tabular shrink-0 ${row.disabled ? "text-muted" : "text-text"}`}>
        {sizeLabel(row.bytes, shared)}
      </span>
      {row.expandable ? (
        <button
          type="button"
          tabIndex={-1}
          aria-label={row.expanded ? `Hide items in ${label}` : `Show items in ${label}`}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand();
          }}
          className="grid size-6 place-items-center rounded-row hover:bg-surface"
        >
          <Chevron open={row.expanded} />
        </button>
      ) : (
        <span className="size-6" />
      )}
    </div>
  );
}

interface ItemProps {
  row: ItemRow;
  active: boolean;
  audience: Audience;
  home: string;
  onToggleCheck: () => void;
  onOpen: () => void;
}

export function ItemRowView({ row, active, audience, home, onToggleCheck, onOpen }: ItemProps) {
  const { item } = row;
  const name = itemName(item);
  const detail = [
    item.lastUsed !== null ? `Last used ${formatAgo(item.lastUsed)}` : null,
    item.blockedReason,
    item.project?.active ? "active project" : null,
    audience === "developer" && item.path ? tildify(item.path, home) : null,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <div className={`${rowClass(active)} pl-9`}>
      <Checkbox
        state={row.checked ? "all" : "none"}
        disabled={!item.cleanable}
        label={`${name}, ${speakBytes(item.bytes)}`}
        onToggle={onToggleCheck}
      />
      <span className="min-w-0 flex-1">
        <button
          type="button"
          tabIndex={-1}
          onClick={(e) => {
            e.stopPropagation();
            onOpen();
          }}
          className="block max-w-full truncate text-left text-text hover:underline"
        >
          {name}
        </button>
        <span className="block truncate text-xs text-muted">{detail}</span>
      </span>
      <span className={`tabular shrink-0 pr-8 ${item.cleanable ? "text-text" : "text-muted"}`}>
        {sizeLabel(item.bytes, item.mayShareBlocks)}
      </span>
    </div>
  );
}

export function SkeletonRowView() {
  return (
    <div className="flex h-full items-center gap-3 px-2" aria-hidden="true">
      <span className="size-4 rounded-[4px] bg-raised" />
      <span className="size-7 rounded-row bg-raised" />
      <span className="flex-1 space-y-1.5">
        <span className="skeleton block h-3 w-2/3 rounded-full" />
        <span className="skeleton block h-2.5 w-1/3 rounded-full" />
      </span>
      <span className="skeleton h-3 w-12 rounded-full" />
    </div>
  );
}
