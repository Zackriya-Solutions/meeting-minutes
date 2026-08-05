import { createContext, useContext } from "react"
import PropTypes from "prop-types"

import * as styles from "./Skeleton.module.css"

// must match the wave animation duration in Skeleton.module.css
const WAVE_PERIOD_MS = 1800
// how long the crest takes to travel one pixel down the page
const WAVE_MS_PER_PX = 1.3

// null — no provider above, boolean — provider state
const SkeletonContext = createContext(null)

export const useSkeletonContext = () => useContext(SkeletonContext)

// Phase-locks the wave to the element's document position so the crest
// sweeps top to bottom across all bars regardless of mount order. Reads
// are batched into a single frame to avoid interleaving with the writes.
let queue = []

const flushWavePhases = () => {
    const nodes = queue
    queue = []
    const now = performance.now()
    const phases = nodes.map((node) => {
        const y = node.getBoundingClientRect().top + window.scrollY
        const offset = (now - y * WAVE_MS_PER_PX) % WAVE_PERIOD_MS
        return -((offset + WAVE_PERIOD_MS) % WAVE_PERIOD_MS)
    })
    nodes.forEach((node, i) => {
        node.style.setProperty("--wave-phase", `${Math.round(phases[i])}ms`)
    })
}

// Attach as a ref on redacted elements: schedules the wave-phase
// measurement for the node
export const waveRef = (node) => {
    if (!node) return
    if (queue.length === 0) requestAnimationFrame(flushWavePhases)
    queue.push(node)
}

// Redaction classes for non-text surfaces (avatars); pair with waveRef
// on the same element
export const useRedactionClassName = (active) =>
    active ? `${styles.redaction} ${styles.active}` : ""

// Bars with no placeholder children fall back to this width, so screens
// can render with empty data and still get sensible skeletons
const DEFAULT_WIDTH_CH = 10

// Inline redaction bar for text: children render transparent under the bar,
// so layout matches the loaded state exactly. Stays mounted while inactive
// to animate the reveal
export const Redaction = ({ active, width, children }) => {
    const hasContent = children != null && children !== ""
    const barWidth =
        width ?? (!hasContent && active ? DEFAULT_WIDTH_CH : undefined)

    return (
        <span
            ref={active ? waveRef : undefined}
            className={`
                ${styles.redaction}
                ${active ? styles.active : ""}
                ${barWidth ? styles.sized : ""}`}
            style={barWidth ? { width: `${barWidth}ch` } : undefined}
        >
            {hasContent ? children : "\u00A0"}
        </span>
    )
}

Redaction.propTypes = {
    active: PropTypes.bool,
    width: PropTypes.number,
    children: PropTypes.node,
}

// Non-text redaction surface: a sized element that shimmers like the text bars,
// with its shape (size / radius) supplied by `className`. Redacts in step with
// an enclosing Skeleton provider; a bare block with no provider defaults to
// redacted, since skeleton screens only render these in loading states. Pass
// `active` to force the state, `as` to change the element (e.g. "span").
export const SkeletonBlock = ({ className = "", as: Tag = "div", active }) => {
    const context = useSkeletonContext()
    const on = active ?? context ?? true
    const redactionClassName = useRedactionClassName(on)
    return (
        <Tag
            ref={on ? waveRef : undefined}
            className={`${className} ${redactionClassName}`.trim()}
        />
    )
}

SkeletonBlock.propTypes = {
    className: PropTypes.string,
    as: PropTypes.elementType,
    active: PropTypes.bool,
}

const Skeleton = ({ active = true, children }) => (
    <SkeletonContext.Provider value={Boolean(active)}>
        {children}
    </SkeletonContext.Provider>
)

Skeleton.propTypes = {
    active: PropTypes.bool,
    children: PropTypes.node,
}

export default Skeleton
