export function isUnicode(char) {
    return /^[\p{L}\p{N}]*$/u.test(char)
}
