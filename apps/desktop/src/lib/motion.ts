import type { Transition } from "motion/react";

// Spec §5.8: springs for spatial changes, short ease-out fades.
export const spring: Transition = { type: "spring", stiffness: 380, damping: 34 };
export const fade: Transition = { duration: 0.18, ease: [0.16, 1, 0.3, 1] };
