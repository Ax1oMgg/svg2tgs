/**
 * Имя файла, которое обрезается посередине: «Монтажная обла… 12.svg».
 * У файлов одной серии одинаковое начало, а различает их конец — номер
 * и расширение, поэтому конец всегда виден. Полное имя — в подсказке.
 */
export function FileName({ name, className }: { name: string; className?: string }) {
  const [head, tail] = splitName(name);
  return (
    <span className={`file-name${className ? ` ${className}` : ""}`} title={name}>
      <span className="file-name__head">{head}</span>
      <span className="file-name__tail">{tail}</span>
    </span>
  );
}

/** Хвост — от последнего разделителя (пробел, _, -), если он недалеко от конца, иначе последние 10 символов. */
export function splitName(name: string): [string, string] {
  const sep = Math.max(name.lastIndexOf(" "), name.lastIndexOf("_"), name.lastIndexOf("-"));
  const cut = sep > 0 && name.length - sep <= 14 ? sep : Math.max(0, name.length - 10);
  return [name.slice(0, cut), name.slice(cut)];
}
