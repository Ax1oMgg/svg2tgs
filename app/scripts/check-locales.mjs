// Проверка переводов (src/locales/*.json) — запускается перед сборкой.
// Перевод — только данные: объект «ключ → строка». В каждом файле те же
// ключи, что в ru.json (эталон), и в каждой строке те же подстановки {имя}.
// Ключи на «_» — служебные (_language — название языка на нём самом).
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = join(import.meta.dirname, "../src/locales");
const read = (f) => JSON.parse(readFileSync(join(dir, f), "utf8"));
const params = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

const base = read("ru.json");
const errors = [];
for (const file of readdirSync(dir).filter((f) => f.endsWith(".json"))) {
  if (!/^[a-z]{2,3}(-[A-Z][A-Za-z]{1,3})?\.json$/.test(file)) {
    errors.push(`${file}: имя файла — код языка, например de.json или pt-BR.json`);
    continue;
  }
  let data;
  try {
    data = read(file);
  } catch (e) {
    errors.push(`${file}: не JSON — ${e.message}`);
    continue;
  }
  if (typeof data !== "object" || data === null || Array.isArray(data)) {
    errors.push(`${file}: ожидается объект «ключ → строка»`);
    continue;
  }
  if (typeof data._language !== "string" || !data._language.trim()) errors.push(`${file}: нет _language`);
  for (const [k, v] of Object.entries(data)) {
    if (typeof v !== "string") errors.push(`${file}: ${k} — не строка`);
    else if (!(k in base)) errors.push(`${file}: лишний ключ ${k}`);
    else if (!k.startsWith("_") && params(v) !== params(base[k]))
      errors.push(`${file}: ${k} — подстановки {${params(v)}}, а нужно {${params(base[k])}}`);
  }
  for (const k of Object.keys(base)) if (!k.startsWith("_") && !(k in data)) errors.push(`${file}: нет ключа ${k}`);
}
if (errors.length) {
  console.error(`Переводы с ошибками:\n  ${errors.join("\n  ")}`);
  process.exit(1);
}
console.log("Переводы в порядке.");
