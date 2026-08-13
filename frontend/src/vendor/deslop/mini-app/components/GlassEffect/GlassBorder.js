import PropTypes from "prop-types"
import * as styles from "./GlassBorder.module.css"

const GlassBorder = ({ className = "", muted = false }) => {
    return (
        <div
            className={`${styles.glassBorder} ${
                muted ? styles.muted : ""
            } ${className}`}
            aria-hidden="true"
        />
    )
}

GlassBorder.propTypes = {
    className: PropTypes.string,
    muted: PropTypes.bool,
}

export default GlassBorder
