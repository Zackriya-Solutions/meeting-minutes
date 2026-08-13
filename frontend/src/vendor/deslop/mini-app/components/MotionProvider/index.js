import PropTypes from "prop-types"
import { LazyMotion, domMax } from "motion/react"

const MotionProvider = ({ children, strict = true }) => {
    return (
        <LazyMotion features={domMax} strict={strict}>
            {children}
        </LazyMotion>
    )
}

MotionProvider.propTypes = {
    children: PropTypes.node,
    strict: PropTypes.bool,
}
export default MotionProvider
