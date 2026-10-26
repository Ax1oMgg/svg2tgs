import { useEffect, useRef, useState } from "react";

interface Props<T extends string> {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}

/**
 * Выпадающий список в стиле интерфейса (вместо системного `<select>`):
 * поле как кнопка Default с шевроном, меню — карточка с тенью, выбранный
 * пункт — с галочкой. Клавиатура: стрелки, Enter, Esc; щелчок мимо закрывает.
 */
export function Dropdown<T extends string>({ label, value, options, onChange }: Props<T>) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  const openMenu = () => {
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    setOpen(true);
  };
  const pick = (v: T) => {
    onChange(v);
    setOpen(false);
  };

  return (
    <div
      ref={root}
      className="dropdown"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          // Esc закрывает меню, а не окно, в котором оно лежит.
          e.preventDefault();
          e.stopPropagation();
          setOpen(false);
        } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          if (!open) return openMenu();
          const step = e.key === "ArrowDown" ? 1 : -1;
          setActive((a) => (a + step + options.length) % options.length);
        } else if ((e.key === "Enter" || e.key === " ") && open) {
          e.preventDefault();
          pick(options[active].value);
        }
      }}
    >
      <button
        type="button"
        className={`btn btn--default dropdown__field${open ? " dropdown__field--open" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={label}
        onClick={() => (open ? setOpen(false) : openMenu())}
      >
        <span>{current?.label}</span>
        <svg className="dropdown__chevron" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      {open && (
        <ul className="dropdown__menu" role="listbox" aria-label={label}>
          {options.map((o, i) => (
            <li
              key={o.value}
              role="option"
              aria-selected={o.value === value}
              className={`dropdown__item${i === active ? " dropdown__item--active" : ""}`}
              onMouseEnter={() => setActive(i)}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => pick(o.value)}
            >
              <span>{o.label}</span>
              <svg className="dropdown__check" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M20 6 9 17l-5-5" />
              </svg>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
