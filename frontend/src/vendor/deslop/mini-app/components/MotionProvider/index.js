import PropTypes from "prop-types"
import { LazyMotion, domMax } from "motion/react"

const MotionProvider = ({ children }) => {
    return (
        <LazyMotion features={domMax} strict>
            {children}
        </LazyMotion>
    )
}

MotionProvider.propTypes = {
    children: PropTypes.node,
}
export default MotionProvider
