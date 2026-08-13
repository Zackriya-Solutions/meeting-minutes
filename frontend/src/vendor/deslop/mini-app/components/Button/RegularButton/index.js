import PropTypes from "prop-types"
import * as m from "motion/react-m"

import { GlassBorder } from "../../GlassEffect"
import Text from "../../Text"
import Skeleton, {
    useSkeletonContext,
    useRedactionClassName,
    waveRef,
} from "../../Skeleton"
import * as styles from "./RegularButton.module.css"
import { useSkin } from "../../../hooks/DeviceProvider"

/**
 * Pill button. Reuse instead of a raw <button>. Extra props (onClick, etc.)
 * spread onto the underlying motion element.
 * @param {object} props
 * @param {"filled"|"outlined"} props.variant
 * @param {string} props.label
 * @param {boolean} [props.isShine] Sweep highlight; only affects `filled`.
 * @param {boolean} [props.isFill] Stretch to fill the container width.
 * @param {"regular"|"medium"|"semibold"|"bold"} [props.labelWeight] Label weight.
 * @example
 * <RegularButton variant="filled" label="Pay" onClick={onPay} isFill />
 */
export const RegularButton = ({
    variant,
    label,
    labelWeight = "semibold",
    isShine = false,
    isFill = false,
    ...props
}) => {
    const { isApple } = useSkin()
    const skeleton = Boolean(useSkeletonContext())
    const redactionClassName = useRedactionClassName(skeleton)

    const dynamicProps = {
        ...(isFill && { "data-fill": true }),
        ...(variant === "filled" &&
            isShine &&
            !skeleton && { "data-shine": true }),
    }

    const label_ = (
        <Text variant="body" weight={labelWeight}>
            {label}
        </Text>
    )

    return (
        <m.div
            ref={skeleton ? waveRef : undefined}
            className={`${styles.button} ${styles[variant]} ${
                skeleton ? styles.skeleton : ""
            } ${redactionClassName}`}
            {...(isApple && !skeleton && { whileTap: { scale: 1.02 } })}
            {...dynamicProps}
            {...props}
        >
            {variant === "filled" && !skeleton && <GlassBorder />}
            {skeleton ? (
                <Skeleton active={false}>{label_}</Skeleton>
            ) : (
                label_
            )}
        </m.div>
    )
}

RegularButton.propTypes = {
    variant: PropTypes.string,
    label: PropTypes.string,
    labelWeight: PropTypes.oneOf(["regular", "medium", "semibold", "bold"]),
    isShine: PropTypes.bool,
    isFill: PropTypes.bool,
}
