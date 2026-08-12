import { createContext, useContext } from "react"

const CellStackContext = createContext(false)

export const useCellStack = () => useContext(CellStackContext)

export default CellStackContext
