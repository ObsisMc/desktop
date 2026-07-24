/**
 * Extracts up to two initials from a display name.
 * "Eric Wang" -> "EW", "Ada" -> "A", "" -> "".
 */
export function getInitials(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) return "";
  const parts = trimmed.split(/\s+/).filter(Boolean);
  const first = parts[0] ?? "";
  // Use the last token for the second initial so "Eric Q. Wang" -> "EW".
  const second = parts.length > 1 ? parts[parts.length - 1] : "";
  return (first.charAt(0) + second.charAt(0)).toUpperCase();
}
