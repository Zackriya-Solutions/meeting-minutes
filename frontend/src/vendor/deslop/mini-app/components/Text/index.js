import PropTypes from "prop-types"

import {
    IconArrowDown as ArrowDownIcon,
    IconArrowUp as ArrowUpIcon,
    IconChevronRight as ChevronRightIcon,
} from "../../../primitives/material-symbols-react"
import { Redaction, useSkeletonContext } from "../Skeleton"
import * as styles from "./Text.module.css"

const variantMap = {
    title1: "title32",
    title2: "title24",
    title3: "title20",
    body: "body",
    callout: "body",
    subheadline1: "subtitle",
    subheadline2: "subtitle",
    footnote: "subtitle",
    caption1: "caption",
    caption2: "caption",
    overline: "caption",
}

export const Text = ({
    as: DirectComponent,
    variant = "body",
    weight,
    rounded,
    skeleton,
    caps,
    chevron,
    arrow,
    children,
    className,
    ...rest
}) => {
    const contextRedacted = useSkeletonContext()
    const Component = DirectComponent || "div"
    const active =
        skeleton !== undefined ? Boolean(skeleton) : Boolean(contextRedacted)
    const redact = skeleton !== undefined || contextRedacted !== null
    const width = typeof skeleton === "number" ? skeleton : undefined
    const content = redact ? (
        <Redaction active={active} width={width}>
            {children}
        </Redaction>
    ) : (
        children
    )
    const ArrowIcon = arrow?.direction === "down" ? ArrowDownIcon : ArrowUpIcon

    return (
        <Component
            {...rest}
            className={`${styles.text} ${styles[variantMap[variant] || "body"]} ${className || ""}`}
            data-variant={variant}
            data-weight={weight}
            data-rounded={rounded || undefined}
            data-caps={caps || undefined}
            data-skeleton={active || undefined}
        >
            {arrow?.direction && (
                <ArrowIcon className={styles.icon} size="1.25em" />
            )}
            {content}
            {chevron && (
                <ChevronRightIcon className={styles.icon} size="1.25em" />
            )}
        </Component>
    )
}

Text.propTypes = {
    as: PropTypes.elementType,
    variant: PropTypes.string,
    weight: PropTypes.oneOf(["regular", "medium", "semibold", "bold"]),
    rounded: PropTypes.bool,
    skeleton: PropTypes.oneOfType([PropTypes.bool, PropTypes.number]),
    caps: PropTypes.bool,
    chevron: PropTypes.bool,
    arrow: PropTypes.shape({ direction: PropTypes.oneOf(["up", "down"]) }),
    children: PropTypes.node,
    className: PropTypes.string,
}

export default Text
