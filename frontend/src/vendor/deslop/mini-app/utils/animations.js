export const EASING = {
    MATERIAL_STANDARD: [0.26, 0.08, 0.25, 1],
    QUINT_OUT: [0.23, 1, 0.32, 1],
    LINEAR: "linear",
    EASE_IN_OUT: "easeInOut",
}

export const DURATION = {
    FAST: 150,
    NORMAL: 200,
    SLOW: 350,
    BALANCE_ANIMATION: 850,
    OPACITY: 200,
}

export const SPRING = {
    APPLE: { type: "spring", stiffness: 640, damping: 40 },
    MATERIAL: { type: "spring", stiffness: 800, damping: 60, mass: 1 },
    DROPDOWN: { type: "spring", stiffness: 500, damping: 32 },
    SNAP: { type: "spring", stiffness: 120, damping: 20 },
    GENTLE: { type: "spring", stiffness: 500, damping: 40 },
    SNACKBAR: { type: "spring", stiffness: 280, damping: 26 },
    MODAL: { type: "spring", stiffness: 250, damping: 30 },
}

export const TRANSITIONS = {
    MATERIAL_STANDARD: {
        ease: EASING.MATERIAL_STANDARD,
        duration: DURATION.NORMAL / 1000,
    },
    MORPH: {
        duration: 0.25,
        type: "spring",
        bounce: 0,
        opacity: { duration: 0.35, type: "spring", bounce: 0 },
    },
}
