export const getColorToken = (token) => {
  if (typeof document === "undefined") return "";
  return getComputedStyle(document.documentElement)
    .getPropertyValue(`--${token}`)
    .trim();
};
