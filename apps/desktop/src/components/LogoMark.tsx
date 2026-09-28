interface LogoMarkProps {
  size?: number;
  className?: string;
}

/** The JClean mark: a small disk map with one cell cleared. */
export function LogoMark({ size = 24, className }: LogoMarkProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
      {...(className ? { className } : {})}
    >
      <rect x="2" y="2" width="10.5" height="20" rx="2" fill="currentColor" />
      <rect x="14.5" y="2" width="7.5" height="10" rx="2" fill="currentColor" opacity="0.6" />
      <rect
        x="15.25"
        y="14.75"
        width="6"
        height="6.5"
        rx="1.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeDasharray="2 1.6"
      />
    </svg>
  );
}
