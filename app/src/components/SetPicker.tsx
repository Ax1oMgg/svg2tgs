import { useState } from "react";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { TgStatus } from "../types";
import { Dropdown } from "./Dropdown";

/** Пункт списка «Другой набор…» — ввод адреса вручную. */
const OTHER = "__other__";

interface Props {
  lang: Lang;
  status: TgStatus;
  /** Начало адреса выбранного набора (без `_by_<бот>`); "" — не выбран. */
  stem: string;
  /** Выбран набор из списка — сразу искать его. */
  onPick: (stem: string) => void;
  /** Ввод адреса вручную: `stem` меняется, искать — по «Найти» или Enter. */
  onType: (stem: string) => void;
  onFind: () => void;
  finding: boolean;
  disabled?: boolean;
}

/** Начало адреса из полного имени набора. */
export function stemOf(name: string, bot: string | null): string {
  const suffix = `_by_${bot ?? ""}`.toLowerCase();
  return name.toLowerCase().endsWith(suffix) ? name.slice(0, -suffix.length) : name;
}

/**
 * Выбор набора бота: выпадающий список наборов, в которые программа уже
 * выгружала (Bot API не отдаёт список наборов бота), и «Другой набор…» —
 * адрес вручную. Без запомненных наборов — сразу поле адреса.
 */
export function SetPicker({ lang, status, stem, onPick, onType, onFind, finding, disabled }: Props) {
  const t = strings[lang];
  const known = status.sets.map((s) => ({ value: stemOf(s.name, status.bot), title: s.title, kind: s.kind }));
  const [other, setOther] = useState(() => known.length === 0 || (stem !== "" && !known.some((k) => k.value === stem)));
  const options = [
    ...known.map((k) => ({ value: k.value, label: t.pubSetLabel(k.title, k.value) })),
    { value: OTHER, label: t.pubSetOther },
  ];

  return (
    <div className="set-picker">
      {known.length > 0 && (
        <Dropdown
          label={t.pubSet}
          value={other ? OTHER : stem || OTHER}
          options={options}
          onChange={(v) => {
            if (v === OTHER) {
              setOther(true);
              onType("");
            } else {
              setOther(false);
              onPick(v);
            }
          }}
        />
      )}
      {other && (
        <div className="row">
          <span className="pub__address">
            <span className="pub__affix">t.me/…/</span>
            <input
              type="text"
              value={stem}
              spellCheck={false}
              aria-label={t.pubSetOtherHint}
              title={t.pubSetOtherHint}
              disabled={disabled}
              onChange={(e) => onType(e.target.value.replace(/[^A-Za-z0-9_]/g, ""))}
              onKeyDown={(e) => e.key === "Enter" && onFind()}
            />
            <span className="pub__affix">_by_{status.bot}</span>
          </span>
          <button type="button" className="btn btn--default" disabled={!stem.trim() || finding || disabled} onClick={onFind}>
            {finding ? t.pubFinding : t.pubFind}
          </button>
        </div>
      )}
    </div>
  );
}
