export function rgbToHex(red: number, green: number, blue: number): string {
  const toHex = (value: number) => value.toString(16).padStart(2, "0");
  return `#${toHex(red)}${toHex(green)}${toHex(blue)}`;
}
