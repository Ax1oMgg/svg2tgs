import { useEffect, useRef, useState } from "react";

export interface ToastData {
  text: string;
  /** Кнопка действия: «Отменить», «Открыть папку». */
  action?: { label: string; run: () => void };
  /** Через сколько мс закрыться само; без него — висит до «×». */
  timeout?: number;
  /** Закрыться при первом действии пользователя вне уведомления
   *  (щелчок, клавиша) — для сводок, которые не нужно держать. */
  dismissOnAction?: boolean;
}

/**
 * Уведомление внизу окна. С таймером — полоска убывает, при наведении
 * таймер на паузе; по окончании уведомление закрывается само.
 */
export function Toast({ toast, closeLabel, onClose }: { toast: ToastData; closeLabel: string; onClose: () => void }) {
  const [left, setLeft] = useState(1);
  const paused = useRef(false);
  const close = useRef(onClose);
  close.current = onClose;

  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!toast.dismissOnAction) return;
    const onAction = (e: Event) => {
      if (box.current?.contains(e.target as Node)) return;
      close.current();
    };
    // Подписка — после текущего события (щелчок «Экспортировать» не в счёт).
    const id = setTimeout(() => {
      window.addEventListener("pointerdown", onAction, true);
      window.addEventListener("keydown", onAction, true);
    }, 0);
    return () => {
      clearTimeout(id);
      window.removeEventListener("pointerdown", onAction, true);
      window.removeEventListener("keydown", onAction, true);
    };
  }, [toast]);

  useEffect(() => {
    if (!toast.timeout) return;
    setLeft(1);
    const step = 50;
    const id = setInterval(() => {
      if (paused.current) return;
      setLeft((l) => {
        const next = l - step / toast.timeout!;
        if (next <= 0) {
          clearInterval(id);
          setTimeout(() => close.current(), 0);
        }
        return Math.max(0, next);
      });
    }, step);
    return () => clearInterval(id);
  }, [toast]);

  return (
    <div
      ref={box}
      className="toast"
      role="status"
      onMouseEnter={() => (paused.current = true)}
      onMouseLeave={() => (paused.current = false)}
    >
      <span className="toast__text">{toast.text}</span>
      {toast.action && (
        <button
          type="button"
          className="btn btn--link toast__action"
          onClick={() => {
            toast.action!.run();
            onClose();
          }}
        >
          {toast.action.label}
        </button>
      )}
      <button type="button" className="toast__close" aria-label={closeLabel} onClick={onClose}>
        ×
      </button>
      {toast.timeout && <span className="toast__timer" style={{ width: `${left * 100}%` }} aria-hidden="true" />}
    </div>
  );
}
