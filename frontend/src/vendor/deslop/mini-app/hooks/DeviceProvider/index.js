import { createContext, useContext } from "react"
import PropTypes from "prop-types"

const APPLE_SKIN = {
    skin: "apple",
    isApple: true,
    isMaterial: false,
    setSkin: () => {},
}

const SkinContext = createContext(APPLE_SKIN)

export const useSkin = () => {
    const value = useContext(SkinContext)
    return value || APPLE_SKIN
}

export default function DeviceProvider({ children }) {
    return (
        <SkinContext.Provider value={APPLE_SKIN}>
            {children}
        </SkinContext.Provider>
    )
}

DeviceProvider.propTypes = {
    children: PropTypes.node,
}
