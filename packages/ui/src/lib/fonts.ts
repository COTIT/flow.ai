import localFont from "next/font/local"

/** Poppins, the brand typeface (OFL, see ../fonts/OFL.txt). Put `fontSans.variable` on <html>. */
export const fontSans = localFont({
  src: [
    { path: "../fonts/Poppins-Regular.ttf", weight: "400", style: "normal" },
    { path: "../fonts/Poppins-Italic.ttf", weight: "400", style: "italic" },
    { path: "../fonts/Poppins-Medium.ttf", weight: "500", style: "normal" },
    { path: "../fonts/Poppins-SemiBold.ttf", weight: "600", style: "normal" },
    { path: "../fonts/Poppins-Bold.ttf", weight: "700", style: "normal" },
    { path: "../fonts/Poppins-ExtraBold.ttf", weight: "800", style: "normal" },
  ],
  variable: "--font-sans",
  display: "swap",
})
