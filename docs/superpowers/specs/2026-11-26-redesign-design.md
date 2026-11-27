# mglaroute-vpn-client — redesign (UI shell, animate-ui, portrait-ready)

Status: proposed (awaiting operator approval)

## Intent / why

Текущий клиент — плоский светлый интерфейс: горизонтальные табы в шапке,
одна колонка карточек, таблица серверов, стоковый Tailwind 3, ноль анимаций,
окно зафиксировано 900×640. Он работает, но не выглядит как продукт и не
готов к портретным экранам. Оператор хочет:

1. Визуальный язык уровня референса/Wired: тёмный стеклянный интерфейс,
   анимации на animate-ui, выразительная, но не шумная подача.
2. Переосмысление каркаса: навигация, список серверов и состояние подключения
   должны быть тремя читаемыми зонами, как на VPN-референсе.
3. Адаптивность под portrait-экраны immediately, так как приложение
   кроссплатформенное.

Решение оператора: поднять стек клиента; в этом плане **не** включать
Android/iOS Tauri-таргеты — только подготовить UI к ним.

## Goals

- Поднять стек: React 18→19, Tailwind 3→4, добавить `motion` v12 и
  `@animate-ui/react` v2.4.x.
- Внедрить тёмную тему в стиле Wired (OKLCH-токены, стеклянные карточки,
  Geist-шрифты) с акцентом `#ff617b` и **зелёным** состоянием подключения
  (без жёлтого свечения из референса).
- Собрать каркас «три зоны» на широком экране и корректно сложить его в одну
  колонку с нижней навигацией на portrait.
- Сохранить всё поведение и контракты: active server через power-кнопку,
  event-driven подсветку probe, три независимых busy-guard, system-proxy
  connect/disconnect, все Tauri-команды и SQLite-схему.
- Обновить тесты и чек-лист под новую разметку; добавить проверку адаптива на
  трёх ширинах.

## Non-goals / anti-goals

- Не добавлять Android/iOS таргеты, Rust-платформенные trait'ы, TUN, anti-DPI,
 Clash/sing-box импорт, трафик/LOCAL IP, login в панель сервера.
- Не менять backend, контракты, события, правила active server и auto-reconnect.
- Не выдумывать данные: флаги стран,LOCAL IP, счётчики трафика, uptime —
  показываем только то, что реально приходит из стоуров/команд.
- Анти-цели: Constant glow/Neon-перегрузка, бесконечные loop-анимации,
  decorative motion speed > 300ms на повторяющихся элементах, ложные
  индикаторы (optimistic «connected»), hidden navigation без aria/focus.

## Constraints

- Tauri v2 + Vite + React; сейчас `apps/desktop/src-tauri/tauri.conf.json`
  задаёт окно 900×640 без `resizable`/min-size. Для проверки адаптива окно
  должно быть resizable с min-size; дефолт увеличивается до 1280×800, чтобы
  широкий режим (≥1280px) был виден при запуске.
- Wired использует `motion ^12.23.26`, Tailwind 4, React 19 и **вендоренные**
  animate-ui-примитивы (~40 файлов в `frontend/src/components/animate-ui/`),
  а не npm-пакет. Клиент изначально на React 18 + Tailwind 3.
- Сейчас есть только Radix `@radix-ui/react-dialog` и `@radix-ui/react-switch`;
  `packages/ui` — пустой стаб, все примитивы локальные в
  `apps/desktop/src/components/ui/`.
- Исходные тесты (`App`, `ServerTable`, `ConnectionPanel`, Stores) проверяют
  текущие классы/лейблы. Перенос разметки требует их обновления.
- Должны сохраниться точные семантики из
  `2026-11-26-current-server-ui-design.md` и
  `2026-11-26-probe-ui-event-driven-design.md`.

## Approaches considered

1. **Три зоны → одна колонка (chosen).** Desktop: левая навигация, центр —
   список серверов, справа — состояние подключения. Portrait: статус-hero
   сверху, список карточками, нижняя навигация. Сохраняем плотность таблицы и
   сходство с VPN-референсом; цена — нужен продуманный breakpoint.
2. **Wired-колонка.** Одна центрированная колонка, логотип, hero подключения,
   табы, карточки серверов. Проще в адаптиве и ближе к скриншоту Wired, но
   теряет плотность списка и «операторский» характер приложения.
3. **Айкон-рейл + верхний статус.** Узкий рейл иконок, рабочая область одна,
   статус — липкая полоса. Больше места списку на 900px, но меньше сходства
   с референсом и статус постоянно отвлекает.

Выбрал оператор: **вариант 1**.

## Decisions

### Breakpoints и каркас

| Ширина viewport | Каркас |
|---|---|
| ≥1280px | Три зоны: nav rail 240px / server list flex / connection panel 320px. |
| 768–1279px | Две зоны: icon rail 64px + контент; connection panel → верхний hero (collapsible). |
| <768px (portrait) | Одна колонка: connection hero наверху, список серверов карточками, нижняя навигация 5 пунктов. |

- Навигация остаётся на `useState<Page>` без router — смена только визуального
  слоя; нижняя и боковая навигация используют одно и то же состояние.
- Центральная зона меняется по выбранной странице; правая панель подключения
  видна на всех пяти страницах. Dashboard — короткое саммари в центре
  (активный сервер, счётчики подписок/серверов, last test cycle); полный
  список серверов остаётся на странице Servers. Power-кнопка активного сервера
  находится только в списке серверов — правая панель управляет глобальным
  Connect/Disconnect, дублирования нет.
- Нижняя навигация: Dashboard / Servers / Subscriptions / Services / Settings.
  Иконки `lucide-react`, подписи, `aria-current="page"`, тач-цели ≥44px,
  учёт `env(safe-area-inset-bottom)`.
- Диалоги на <768px — bottom sheet; на широких — центрированный dialog с
  blur-overlay и лёгким perspective, как в Wired.
- Окно: `resizable: true`, `minWidth: 360`, `minHeight: 640`,
  default 1280×800 (совпадает с порогом трёх зон).
- На 768–1279px connection hero развёрнут по умолчанию; состояние
  сворачивания живёт только в session state `AppShell` и сбрасывается при
  перезапуске.

### Визуальная система

- Принудительно dark: `<html class="dark">`; токены OKLCH как в Wired
  (`--background: oklch(0.145 0 0)`, `--card: oklch(0.205 0 0)`, границы
  `oklch(1 0 0 / 10%)`, радиус 0.625rem).
- Фон: CSS-градиент + мягкий radial-слой, без новых бинарных ассетов.
  Параллакс-фото из Wired не копируем: в репозитории клиента нет картинок,
  а сборка Tauri должна остаться компактной.
- Стекло: `backdrop-blur-lg bg-black/20` на карточках/таб-листе.
- Акцент `#ff617b` — ссылки, активные табы, фокус-кольца.
- Подключение: зелёный `#4ade80` (green-400) для active/connected. **Никакого
  золотого свечения и постоянного pulsing/glow**; только короткий
  scale-pulse в момент успешного connect.
- Шрифты: Geist + Geist Mono через CSS-переменные, без Google Fonts в рантайме
  (шрифты клонируются в проект или остаётся системный стек, если оффлайн-сборка
  не позволяет загрузить файлы — builder call).
- Текст: muted `text-muted-foreground`, моно для IP/адресов, line-clamp для
  длинных имён подписок.

### animate-ui / motion

- Первичная интеграция: npm `@animate-ui/react` v2.4.x + `motion` v12,
  выбранные Radix-primitive peer deps (Tabs, Dialog, Switch, Tooltip,
  ScrollArea/Accordion — по факту использования). Примитивы заменяют импорты
  в локальных `components/ui`, а не удаляют их: API сохраняется.
- Fallback (builder call): если npm-пакет даст peer-конфликт или не экспортирует
  нужный эффект, перенести только нужные файлы из
  `webdev/Wired/frontend/src/components/animate-ui/` (они уже работают на
  Tailwind 4 + motion v12) в `apps/desktop/src/components/animate-ui/`.
- Что анимируем:
  - навигация: скользящий `layoutId`-хайлайт, spring `{stiffness:200,damping:25}`;
  - смена страницы: fade + 8px y, не горизонтальный карусельный track;
  - кнопки: hover scale 1.05, tap 0.95, ripple по точке клика;
  - primary Connect: liquid-fill на hover;
  - Dialog/Sheet: overlay blur, content perspective + scale; на мобильных —
    slide-up sheet;
  - Switch: анимированный thumb;
  - строки серверов: `layout`-анимация пересортировки после probe, статус
    перекрашивается без мигания; testing — тонкий indeterminate штрих.
- Обязательно `useReducedMotion`: при системной настройке все motion-эффекты
  превращаются в мгновенные state-переходы. Никаких бесконечных анимаций,
  кроме тонкого индикатора testing (и он отключается при reduced motion).

### Приложения зон

- **Левая навигация**: логотип, 5 пунктов, внизу компактный блок реального
  статуса подключения (`ConnectionState.status`) + версия. «Cluster status» и
  VPN IP не показываем — их нет в контрактах. В узком режиме — иконки, в
  мобильном — нижний bar.
- **Центр — серверы**: заголовок «Servers», search (фильтр по имени/адресу),
  кнопки Run test / Stop test, tested/total счётчик. На desktop сохраняется
  таблица, но с переработанной визуальной подачей (строки-стеклянные панели,
  power-кнопка, задержка, статус). На <768px таблица заменяется карточками:
  имя/адрес, задержка, статус, power-кнопка, ripple-нажатие.
- **Правая панель — подключение**: большая круглая power-кнопка
  (зелёная при connected), активный сервер с реальным адресом узла
  (`Profile.address`), local proxy port (`ConnectionState.proxyPort`),
  «Last test cycle», footnote про system proxy/TUN/anti-DPI. VPN IP, LOCAL IP,
  трафик и uptime не показываем до появления данных в контракте. Кнопка
  по-прежнему управляет глобальным Connect/Disconnect; power в строке выбирает
  активный сервер и переключает живое соединение — подписи/tooltip'ы обязаны
  это различать.

### Сохранение поведения

- `connection.profileId ?? selectedProfileId` — единственный active id.
- `probe-server` события — единственный источник подсветки testing;
  terminal event сразу патчит `latencyMs`/`reachable`; `checkedAt` не
  синтезируется.
- Три guard'а (`connection_busy`, `probe_busy`, `mutation_busy`) остаются
  независимыми; `probe_busy` не блокирует Connect; Stop test сохраняет
  частичные результаты и пропускает глобальную пересортировку.
- Никаких изменений в Rust/commands/contracts/SQLite.

## Tests / verification

- Обновить vitest-тесты под новую разметку, сохранив проверки семантики:
  `App.test.tsx`, `ServerTable.test.tsx`, `ConnectionPanel.test.tsx`,
  `app.store.test.ts`, `radix-wrappers.test.tsx`.
- Добавить проверку, что при `prefers-reduced-motion` motion-компоненты не
  анимируются (проверять через prop/mock, не через jsdom-анимации).
- Добавить в `docs/e2e-checklist.md` проход на трёх ширинах: 360×640 portrait,
  900×640 (две зоны, текущее окно), 1280×800 (три зоны). Проверять: доступность всех
  пяти разделов, нижнюю навигацию, отсутствие горизонтального скролла,
  читаемость строк серверов, workable Connect/Disconnect и Run/Stop test.
- `NX_DAEMON=false pnpm test` и `NX_DAEMON=false pnpm build`; `cargo test` с
  test-only `TAURI_CONFIG` как описано в AGENTS.md.

## Risks / open items

- npm `@animate-ui/react` может требовать React 19 peer и Tailwind 4 — это
  совпадает с выбранным апгрейдом, но точный список exports нужно проверить
  перед имплементацией (fallback выше).
- Tailwind 4 миграция: удаление `tailwind.config.js`, перенос токенов в CSS,
  переименование утилит (`shadow-sm`→`shadow-xs` и т.п.) в локальных
  component'ах.
- Geist-шрифты: нужно решить, вендорить ли woff2 в репозиторий или оставить
  системный стек (offline-сборка Tauri).
- Отсутствие реальных флагов/статистики в контрактах — не заполнять выдумкой.
- Resize окна не равно мобильному тач-вводу: план готовит UI, но не проверяет
  настоящий Android/iOS.

## What's left to builder

- Проверить exports/peer deps animate-ui и зафиксировать выбор npm vs vendor.
- Перенести токены Wired в `apps/desktop/src/index.css` (Tailwind 4 CSS-first),
  обновить `postcss.config.js`, удалить `tailwind.config.js`.
- Собрать `AppShell` с тремя режимами и нижней навигацией.
- Переделать `pages/*` и `features/*` под новые зоны, сохранив логику.
- Обновить тесты и e2e-чек-лист, прогнать верификацию.
