import { Children } from "react"
import PropTypes from "prop-types"
import * as m from "motion/react-m"
import { Calligraph } from "calligraph"

import Cell from "../Cells"
import { TRANSITIONS } from "../../utils/animations"

import { useCellStack } from "./context"
import * as styles from "./CellStack.module.css"

const TRANSITION = TRANSITIONS.MATERIAL_STANDARD
const LOGO_FRONT = {
    collapsed: { scale: 0.6, x: -6, y: -6 },
    expanded: { scale: 1, x: 0, y: 0 },
}
const LOGO_BEHIND = {
    collapsed: { scale: 0.6, x: 6, y: 6, opacity: 1 },
    expanded: { scale: 0, x: 0, y: 0, opacity: 0 },
}

const readFace = (cell) => {
    const { start, end, children: body } = cell.props
    return {
        start,
        title: body?.props?.title,
        description: body?.props?.description,
        bold: body?.props?.bold,
        value: end?.props?.title,
    }
}

const morph = (text, variant = "text") =>
    typeof text === "string" ? (
        <Calligraph variant={variant} animation="smooth">
            {text}
        </Calligraph>
    ) : (
        text
    )

function Morph({ children }) {
    const expanded = useCellStack()
    const [collapsedFace, expandedFace] = Children.toArray(children).map(readFace)
    const state = expanded ? "expanded" : "collapsed"
    const face = expanded ? expandedFace : collapsedFace
    const description = face.description

    return (
        <Cell
            start={
                <div className={styles.logoStack}>
                    <m.div
                        className={styles.logoFront}
                        variants={LOGO_FRONT}
                        animate={state}
                        transition={TRANSITION}
                    >
                        {expandedFace.start}
                    </m.div>
                    <m.div
                        className={styles.logoBehind}
                        variants={LOGO_BEHIND}
                        animate={state}
                        transition={TRANSITION}
                    >
                        {collapsedFace.start}
                    </m.div>
                </div>
            }
            end={face.value && <Cell.Text title={morph(face.value, "number")} />}
        >
            <Cell.Text
                title={
                    <m.span
                        className={styles.morphLine}
                    >
                        {morph(face.title)}
                    </m.span>
                }
                description={
                    description && (
                        <m.span
                            className={styles.morphLine}
                        >
                            {morph(description)}
                        </m.span>
                    )
                }
                bold={face.bold}
            />
        </Cell>
    )
}

Morph.propTypes = { children: PropTypes.node.isRequired }

export default Morph
