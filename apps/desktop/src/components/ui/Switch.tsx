interface SwitchProps {
  on: boolean;
  label: string;
  onChange: () => void;
  disabled?: boolean;
}

/** An on/off switch; the thumb slides with a transform. */
export function Switch({ on, label, onChange, disabled = false }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={onChange}
      className={`relative h-5 w-9 shrink-0 rounded-full transition-colors disabled:opacity-40 ${on ? "bg-accent" : "bg-line"}`}
    >
      <span
        className={`absolute top-0.5 left-0.5 size-4 rounded-full bg-white shadow transition-transform duration-150 ${
          on ? "translate-x-4" : ""
        }`}
      />
    </button>
  );
}
