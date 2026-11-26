// Toasts (Sonner), in the app's theme: brief messages that float over the page instead of taking room in it.

import { CircleCheck, CircleX, Info, TriangleAlert } from "lucide-react";
import { Toaster as Sonner, type ToasterProps } from "sonner";
import { useThemeMode } from "../../theme";

export function Toaster(props: ToasterProps) {
  const mode = useThemeMode();
  return (
    <Sonner
      theme={mode}
      position="bottom-right"
      closeButton
      icons={{
        success: <CircleCheck className="size-4 text-success" />,
        error: <CircleX className="size-4 text-destructive" />,
        info: <Info className="size-4" />,
        warning: <TriangleAlert className="size-4 text-warning" />,
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      {...props}
    />
  );
}
