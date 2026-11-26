const formatter = new Intl.DateTimeFormat("ru-RU", {
  timeZone: "Europe/Moscow",
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

/** ISO-8601 (including nanosecond) → `ДД.ММ.ГГГГ ЧЧ:ММ:СС`, Europe/Moscow. */
export function formatTimestamp(iso: string): string {
  const parts = Object.fromEntries(
    formatter.formatToParts(new Date(iso)).map(({ type, value }) => [type, value]),
  );
  return `${parts.day}.${parts.month}.${parts.year} ${parts.hour}:${parts.minute}:${parts.second}`;
}
