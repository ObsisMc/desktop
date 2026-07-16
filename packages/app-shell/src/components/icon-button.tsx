import type { FC } from "react";
import { Button, cn } from "@ora/ui";

interface IconButtonProps {
  icon: FC<{ className?: string }>;
  /** Accessible label for the button (it has no visible text). */
  label: string;
  onClick?: () => void;
  variant?: "ghost" | "secondary" | "default";
  className?: string;
  disabled?: boolean;
}

/**
 * A compact square icon-only button built on the @ora/ui Button. Used for
 * toolbar actions (new chat, toggle sidebar, copy, etc.) where a labeled
 * button would be too heavy.
 */
export function IconButton({
  icon: Icon,
  label,
  onClick,
  variant = "ghost",
  className,
  disabled,
}: IconButtonProps) {
  return (
    <Button
      variant={variant}
      size="icon-sm"
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
      className={cn("shrink-0", className)}
    >
      <Icon className="size-[18px] text-muted-foreground" />
    </Button>
  );
}
