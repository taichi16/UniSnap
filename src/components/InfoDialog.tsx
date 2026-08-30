import { useEffect, type ReactNode } from "react";
import { X } from "lucide-react";

interface InfoDialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
}

export default function InfoDialog({ title, onClose, children }: InfoDialogProps) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="info-dialog-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <section className="info-dialog" role="dialog" aria-modal="true" aria-label={title}>
        <header className="info-dialog-header">
          <h2>{title}</h2>
          <button className="info-dialog-close" onClick={onClose} aria-label="關閉">
            <X size={17} />
          </button>
        </header>
        <div className="info-dialog-content">{children}</div>
      </section>
    </div>
  );
}
