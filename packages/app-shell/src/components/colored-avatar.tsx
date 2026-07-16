import { Avatar, AvatarFallback, cn } from "@ora/ui";
import { getAvatarColor, getInitials } from "../lib/avatar";

type AvatarSize = "sm" | "default" | "lg";

interface ColoredAvatarProps {
  name: string;
  size?: AvatarSize;
  className?: string;
}

/**
 * A solid-color initials avatar (e.g. "Eric Wang" -> "EW") rendered on top of
 * the @ora/ui Avatar shell. The background hue is picked deterministically
 * from the name, and the initials use a darker shade of the same hue.
 */
export function ColoredAvatar({ name, size = "default", className }: ColoredAvatarProps) {
  const { bg, fg } = getAvatarColor(name);
  return (
    <Avatar size={size} className={className}>
      <AvatarFallback className={cn("font-semibold", bg, fg)}>
        {getInitials(name)}
      </AvatarFallback>
    </Avatar>
  );
}
