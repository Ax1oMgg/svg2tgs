import { useState } from "react";
import * as backend from "../backend";
import type { Lang } from "../i18n";
import { strings } from "../i18n";
import type { Theme } from "../prefs";
import type { Options } from "../types";

interface Props {
  lang: Lang;
  theme: Theme;
  options: Options;
  onOptions: (o: Options) => void;
  onLang: (l: Lang) => void;
  onTheme: (t: Theme) => void;
  version: string;
}

/** Настройки конвертации и приложения. */
export function SettingsPanel({ lang, theme, options, onOptions, onLang, onTheme, version }: Props) {
  const t = strings[lang];
  const set = (patch: Partial<Options>) => onOptions({ ...options, ...patch });
  const [color, setColor] = useState(
    options.background.kind === "solid" ? options.background.color : "#ffffff",
  );

  return (
    <aside className="settings" aria-label={t.settings}>
      <h2 className="panel-title">{t.settings}</h2>

      <Field label={t.preset}>
        <div className="segmented segmented--wide" role="radiogroup" aria-label={t.preset}>
          {(["sticker", "emoji"] as const).map((p) => (
            <button
              key={p}
              type="button"
              role="radio"
              aria-checked={options.preset === p}
              className="segmented__item"
              onClick={() => set({ preset: p })}
            >
              {p === "sticker" ? t.sticker : t.emoji}
            </button>
          ))}
        </div>
      </Field>

      <Field label={t.padding} value={`${Math.round(options.padding * 100)} %`}>
        <input
          type="range"
          min={0}
          max={20}
          step={1}
          value={Math.round(options.padding * 100)}
          onChange={(e) => set({ padding: Number(e.target.value) / 100 })}
          aria-label={t.padding}
        />
      </Field>

      <Field label={t.fit}>
        <select
          value={options.fit}
          onChange={(e) => set({ fit: e.target.value as Options["fit"] })}
          aria-label={t.fit}
        >
          <option value="contain">{t.fitContain}</option>
          <option value="cover">{t.fitCover}</option>
        </select>
      </Field>

      <Field label={t.background}>
        <div className="row">
          <select
            value={options.background.kind}
            onChange={(e) =>
              set({
                background:
                  e.target.value === "solid" ? { kind: "solid", color } : { kind: "transparent" },
              })
            }
            aria-label={t.background}
          >
            <option value="transparent">{t.transparent}</option>
            <option value="solid">{t.color}</option>
          </select>
          {options.background.kind === "solid" && (
            <input
              type="color"
              value={color}
              aria-label={t.color}
              onChange={(e) => {
                setColor(e.target.value);
                set({ background: { kind: "solid", color: e.target.value } });
              }}
            />
          )}
        </div>
      </Field>

      <Field label={t.optimization}>
        <select
          value={options.optimization.mode}
          onChange={(e) => {
            const mode = e.target.value as Options["optimization"]["mode"];
            set({
              optimization: mode === "manual" ? { mode, precision: 2 } : ({ mode } as Options["optimization"]),
            });
          }}
          aria-label={t.optimization}
        >
          <option value="auto">{t.optAuto}</option>
          <option value="lossless">{t.optLossless}</option>
          <option value="manual">{t.optManual}</option>
        </select>
        {options.optimization.mode === "manual" && (
          <label className="sub">
            {t.decimals}
            <input
              type="number"
              min={0}
              max={6}
              value={options.optimization.precision}
              onChange={(e) =>
                set({
                  optimization: {
                    mode: "manual",
                    precision: Math.max(0, Math.min(6, Number(e.target.value) || 0)),
                  },
                })
              }
            />
          </label>
        )}
      </Field>

      <details className="advanced">
        <summary>{t.advanced}</summary>
        <Field label={t.fps}>
          <input
            type="number"
            min={1}
            max={120}
            placeholder={`60 — ${t.telegramDefault}`}
            value={options.fps ?? ""}
            onChange={(e) => set({ fps: e.target.value ? Number(e.target.value) : null })}
          />
        </Field>
        <Field label={t.duration}>
          <input
            type="number"
            min={0.1}
            max={10}
            step={0.1}
            placeholder="1"
            value={options.duration_secs ?? ""}
            onChange={(e) => set({ duration_secs: e.target.value ? Number(e.target.value) : null })}
          />
        </Field>
      </details>

      <h2 className="panel-title panel-title--spaced">{t.app}</h2>
      <Field label={t.language}>
        <select value={lang} onChange={(e) => onLang(e.target.value as Lang)} aria-label={t.language}>
          <option value="ru">Русский</option>
          <option value="en">English</option>
        </select>
      </Field>
      <Field label={t.theme}>
        <select value={theme} onChange={(e) => onTheme(e.target.value as Theme)} aria-label={t.theme}>
          <option value="system">{t.themeSystem}</option>
          <option value="light">{t.themeLight}</option>
          <option value="dark">{t.themeDark}</option>
        </select>
      </Field>

      <section className="about" aria-label={t.about}>
        <h2 className="panel-title">{t.about}</h2>
        <p>
          svg2tgs{version && ` · ${t.version(version)}`}
          <br />
          {t.aboutProject}
          <br />
          {t.aboutAuthor}
          <br />
          {t.aboutLicense}
        </p>
        <button type="button" className="btn btn--link" onClick={() => void backend.openProjectPage()}>
          {t.projectPage}
        </button>
      </section>
    </aside>
  );
}

function Field({ label, value, children }: { label: string; value?: string; children: React.ReactNode }) {
  return (
    <div className="field">
      <div className="field__label">
        <span>{label}</span>
        {value && <span className="field__value">{value}</span>}
      </div>
      {children}
    </div>
  );
}
