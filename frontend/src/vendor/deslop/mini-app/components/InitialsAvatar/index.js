import PropTypes from "prop-types"

import {
    useSkeletonContext,
    useRedactionClassName,
    waveRef,
} from "../Skeleton"
import { isUnicode } from "../../utils/common"
import { avatarGradients } from "../../../primitives/tokens"

import * as styles from "./InitialsAvatar.module.css"

const InitialsAvatar = ({ size = 40, userId, name }) => {
    const redacted = Boolean(useSkeletonContext())
    const redactionClassName = useRedactionClassName(redacted)
    const gradient = avatarGradients[userId % avatarGradients.length]
    const [firstName = "", lastName = ""] = name.split(" ")
    const background = `linear-gradient(180deg, ${gradient.top} 0%, ${gradient.bottom} 100%)`

    return (
        <div
            ref={redacted ? waveRef : undefined}
            className={`${styles.root} ${redactionClassName}`}
            style={{
                width: size,
                height: size,
                background: redacted ? undefined : background,
                "--font_size": `${Math.round(size / 2.2)}px`,
            }}
        >
            <div
                className={`${styles.initials} ${redacted ? styles.hiddenInitials : ""}`}
            >
                {isUnicode(firstName.charAt(0)) &&
                    firstName.charAt(0).toLocaleUpperCase()}
                {isUnicode(lastName.charAt(0)) &&
                    lastName.charAt(0).toLocaleUpperCase()}
            </div>
        </div>
    )
}

InitialsAvatar.propTypes = {
    size: PropTypes.number,
    userId: PropTypes.number.isRequired,
    name: PropTypes.string.isRequired,
}

export default InitialsAvatar
