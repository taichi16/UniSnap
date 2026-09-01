import { useEffect, useId, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FontOption } from "../editor/fonts";

interface FontPickerProps {
  value: string;
  options: FontOption[];
  loading: boolean;
  error: string | null;
  onChange: (fontFamily: string) => void;
}

interface MenuPosition {
  left: number;
  top?: number;
  bottom?: number;
  maxHeight: number;
}

export default function FontPicker({ value, options, loading, error, onChange }: FontPickerProps) {
  const listId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const selected = useMemo(
    () => options.find((option) => option.value === value) ?? options[0],
    [options, value],
  );
  const [query, setQuery] = useState(selected?.label ?? "");
  const [open, setOpen] = useState(false);
  const [menuPosition, setMenuPosition] = useState<MenuPosition | null>(null);

  useEffect(() => {
    setQuery(selected?.label ?? "");
  }, [selected?.label]);

  const filteredOptions = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized || query === selected?.label) return options;
    return options.filter((option) => option.label.toLocaleLowerCase().includes(normalized));
  }, [options, query, selected?.label]);

  const choose = (option: FontOption) => {
    onChange(option.value);
    setQuery(option.label);
    setOpen(false);
  };

  const openMenu = () => {
    const input = inputRef.current;
    if (!input) return;
    const rect = input.getBoundingClientRect();
    const margin = 8;
    const gap = 5;
    const menuWidth = 240;
    const spaceBelow = window.innerHeight - rect.bottom - gap - margin;
    const spaceAbove = rect.top - gap - margin;
    const openBelow = spaceBelow >= 180 || spaceBelow >= spaceAbove;
    const maxHeight = Math.max(100, Math.min(300, openBelow ? spaceBelow : spaceAbove));
    const left = Math.min(
      Math.max(margin, rect.left),
      Math.max(margin, window.innerWidth - menuWidth - margin),
    );
    setMenuPosition(openBelow
      ? { left, top: rect.bottom + gap, maxHeight }
      : { left, bottom: window.innerHeight - rect.top + gap, maxHeight });
    setOpen(true);
  };

  return (
    <div
      className="font-picker"
      title={error ?? "搜尋並選擇 Windows 已安裝字型"}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setOpen(false);
          setQuery(selected?.label ?? "");
        }
      }}
    >
      <span>字型:</span>
      <div className="font-picker-control">
        <input
          type="text"
          ref={inputRef}
          value={query}
          aria-label="搜尋系統字型"
          aria-expanded={open}
          aria-controls={listId}
          placeholder={loading ? "正在讀取字型…" : "搜尋字型"}
          onFocus={(event) => {
            event.currentTarget.select();
            openMenu();
          }}
          onChange={(event) => {
            setQuery(event.target.value);
            setOpen(true);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setOpen(false);
              setQuery(selected?.label ?? "");
            }
          }}
        />
        {open && menuPosition && createPortal(
          <div
            id={listId}
            className="font-picker-list"
            role="listbox"
            style={{
              left: menuPosition.left,
              top: menuPosition.top,
              bottom: menuPosition.bottom,
              maxHeight: menuPosition.maxHeight,
            }}
          >
            {filteredOptions.length > 0 ? filteredOptions.map((font) => (
              <button
                key={`${font.source}:${font.label}`}
                type="button"
                role="option"
                aria-selected={font.value === value}
                className={font.value === value ? "selected" : ""}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => choose(font)}
              >
                <span className="font-picker-name">{font.label}</span>
                <span className="font-picker-preview" style={{ fontFamily: font.value }} aria-hidden="true">
                  Aa 中文
                </span>
              </button>
            )) : <div className="font-picker-empty">找不到符合的字型</div>}
          </div>,
          document.body,
        )}
      </div>
      {loading && <span className="font-picker-status">載入中</span>}
      {!loading && !error && <span className="font-picker-status">{options.length} 套</span>}
      {error && <span className="font-picker-status error">系統字型讀取失敗</span>}
    </div>
  );
}
