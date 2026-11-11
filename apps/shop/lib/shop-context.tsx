"use client"

import { createContext, useContext } from "react"

export const ShopContext = createContext({ base: "", name: "" })
export const useShop = () => useContext(ShopContext)
