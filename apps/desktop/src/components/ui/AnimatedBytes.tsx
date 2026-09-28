import { animate, useMotionValue, useMotionValueEvent, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { formatBytes } from "../../lib/format";

interface AnimatedBytesProps {
  bytes: number;
  prefix?: string;
  className?: string;
}

/** A size that counts to its new value instead of jumping (spec §5.8). */
export function AnimatedBytes({ bytes, prefix = "", className }: AnimatedBytesProps) {
  const value = useMotionValue(bytes);
  const [shown, setShown] = useState(bytes);
  const reduce = useReducedMotion();

  useMotionValueEvent(value, "change", (v) => {
    setShown(v);
  });

  useEffect(() => {
    if (reduce) {
      value.set(bytes);
      return;
    }
    const controls = animate(value, bytes, { duration: 0.6, ease: [0.16, 1, 0.3, 1] });
    return () => {
      controls.stop();
    };
  }, [bytes, reduce, value]);

  return (
    <span className={`tabular ${className ?? ""}`}>
      {prefix}
      {formatBytes(shown)}
    </span>
  );
}
