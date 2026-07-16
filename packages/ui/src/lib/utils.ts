import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

/** Merges conditional class names while resolving conflicting Tailwind utilities. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}
