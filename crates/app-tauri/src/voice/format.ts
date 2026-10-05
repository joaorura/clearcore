/** pt-* shows a decimal comma; every other language keeps the point. */
function usesDecimalComma(lang: string): boolean {
  return (lang || '').toLowerCase().startsWith('pt');
}

/** Rounds to `digits` decimals and uses the decimal separator of `lang`. */
export function formatDecimalLocale(value: number, digits: number, lang: string): string {
  const f = 10 ** digits;
  const s = (Math.round(value * f) / f).toFixed(digits);
  return usesDecimalComma(lang) ? s.replace('.', ',') : s;
}

/** Seconds with one decimal: "84,3" in pt-BR, "84.3" in en-US. */
export function formatSecondsLocale(seconds: number, lang: string): string {
  return formatDecimalLocale(seconds, 1, lang);
}
