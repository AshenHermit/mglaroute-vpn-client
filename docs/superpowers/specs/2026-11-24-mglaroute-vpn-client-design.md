# mglaroute-vpn-client — дизайн (MVP)

Дата: 2026-11-24. Статус: **approved** (оператор подтвердил 2026-11-24).

## Intent / why

Нужен простой кроссплатформенный VPN-клиент для экосистемы mglaroute. Сервер уже
умеет сканировать подписки, мерить real delay и отдавать публичную выдачу
`GET /api/feed`. Клиент должен забрать на себя то, что сервер не делает: локальный
импорт подписок по URL, локальные Profiling-наборы онлайн-сервисов, периодические
тесты, сортировку и подключение. За эталон импорта/подключения взят v2rayN —
рабочий xray-клиент, но конфигурация должна быть проще и дружелюбнее.

Источники исследования: `../v2rayN/v2rayN`, `../anti-dpi-for-russia`,
`../mglaroute-vpn-server/docs/superpowers/specs/*`, `../mglaroute-vpn-server/backend/src`.

## Goals

1. Desktop Windows: Tauri v2 + React + Zustand + shadcn/Tailwind, pnpm + Nx.
2. Импорт подписок по URL с декодированием в стиле v2rayN (cascading best-effort).
3. Профили онлайн-сервисов: имя + список URL, CRUD, включение/отключение.
4. Включение/отключение подписок + периодический auto-refetch.
5. Периодический auto-test: real delay каждого сервера и доступность всех URL активных
   профилей через тот же прокси-сеанс; авто-сортировка по delay.
6. Авто-переподключение к лучшему серверу после каждого цикла тестов, с cooldown.
7. Включение/отключение VPN через системный прокси Windows без прав администратора.
8. xray и anti-DPI движки включены в build (не скачиваются при первом запуске).

## Non-goals / anti-goals

- Anti-DPI **не входит в MVP**. Зарезервировать трейт, stubs команд и бинарники/лицензии
  в сборке. Полная интеграция — отдельный этап.
- TUN-режим, Linux UI, Android/iOS UI — вне MVP.
- Полные конфиги Clash YAML / sing-box / v2ray JSON как типы подписки (v2rayN
  передаёт их другим ядрам как «сырые» файлы).
- Парсинг `subscription-userinfo` (трафик/expiry) — в v2rayN отсутствует.
- Логин в панель сервера из клиента. Клиент импортирует только публичные URL;
  `GET /api/feed` — публичный, без JWT.
- Никаких секретов (URL подписок, credentials, raw config) в логах и телеметрии.
- Никакого выполнения строк из подписки вне сгенерированного xray config.

## Constraints

- Стек зафиксирован оператором: tauri, vite, react, zustand, shadcn, tailwind, pnpm, nx.
- xray и anti-DPI бинарники **должны быть в build**. Значит build-time скрипт, а не
  first-run download. Источник — GitHub Releases, SHA-256 из `.dgst`, как
  `../mglaroute-vpn-server/backend/scripts/download-xray.mjs`.
- Windows proxy: WinInet `INTERNET_OPTION_PER_CONNECTION_OPTION` + registry fallback
  (`Software\Microsoft\Windows\CurrentVersion\Internet Settings`). Без admin.
  Обязательно восстановить прежнее состояние при disconnect/crash.
- zapret2/Igareck: движок не лежит в репозитории; Windows = `winws2.exe` +
  WinDivert driver (admin, UAC), Linux = `nfqws2` (сборка локально, root + nftables
  NFQUEUE). Каталог строк **не поддерживает список доменов** — только номер стратегии,
  сетевой интерфейс и TCP-порты.
- Лицензии: Igareck wrapper GPL-3.0; zapret2 MIT; WinDivert LGPL-3.0-or-later OR
  GPL-2.0-or-later; cygwin1.dll LGPL + linking exception; LuaJIT MIT; шрифты OFL 1.1.
  При bundled-распространении сохранить `THIRD_PARTY_NOTICES.md` и `licenses/`.
- Real delay по образцу v2rayN: 2 GET, explicit loopback proxy (не ENV-прокси),
  min positive ms, default ping URL `https://www.google.com/generate_204`.
- Проверка сервиса: 2xx на **всех** URL профиля, через тот же локальный прокси.
  Пустой список URL = профиль никогда не проходит. Нет fallback на прямой запрос.

## Approaches considered / решения

| Решение | Варианты | Выбор | Почему |
|---|---|---|---|
| VPN mode MVP | system proxy / TUN | **system proxy** | без драйвера и admin; TUN позже |
| Анти-DPI | обёртка Igareck PS/Python / свой Rust supervisor над bundles / порт 484 стратегий | **отложить, зарезервировать свой Rust supervisor + 3–5 пресетов** | оператор: не в MVP; в сборке только бинарники |
| Доставка ядер | скачивать при первом запуске / bundled в build | **bundled в build** (оператор) | предсказуемость офлайн-запуска |
| Хранилище | JSON-файлы / SQLite | **SQLite (rusqlite)** | объём профилей/результатов, индексы, как v2rayN |
| Тесты delay | один core на профиль / один core с N inbounds на цикл | **один core с N `mixed` inbounds, батч** | меньше spawn, те же результаты |
| Семантика профиля сервисов | только подсветка / требовать прохождения + допуск к autoconnect | **требовать + допуск** (оператор) | «фильтрация серверов» |
| Авто-переподключение | только вручную/по обрыву / после каждого цикла с cooldown | **после цикла с cooldown** (оператор) | свежие замеры → лучший сервер |

## Settled by designer (не требовало оператора)

- **Пул серверов глобальный** (подтверждено оператором). Все `present=true` профили всех активных подписок
  участвуют в тесте и сортировке; `sort_index` глобальный; подключение — одно,
  лучший сервер из пула. `subscription_id` в snapshot нужен только для connect.
- **Старт без автоподключения** (подтверждено оператором). При запуске приложение не подключается само.
  Прошлое состояние системного прокси сохраняется на диске (`proxy_previous.json`),
  чтобы восстановить его после выхода/краша процесса, а не только при fault xray.
- **Пустые/неудачные фильтры.** Нет активных service_profiles → фильтрации нет,
  все серверы допустимы. Нет данных теста → auto-reconnect не срабатывает, ручной
  connect по явно выбранному профилю разрешён. Нет ни одного прошедшего кандидата →
  авто-переподключение снимает соединение.
- **Anti-DPI binaries provenance.** Windows MVP: `bol-van/zapret-win-bundle`,
  pinned commit `6eb463a6758fb48cd101bc55dfd057e6e9d98af1`, sha256
  `5f828239310fed0da587bc09eaa22742615b2a85cf41f55ff28128e22231c6de`. Linux
  (`nfqws2`, сборка из `bol-van/zapret2`) — позже, в Windows-сборку не входит.
- **Concurrency.** Все мутирующие команды сериализуются одним токеном в Rust;
  конфликтующая команда возвращает `Busy` и не запускает вторую сессию ядра/прокси.
- **`direct` fallback** = `freedom` outbound + routing rules для доменов из
  `proxy_exclusions`; не balancer. `proxy_exclusions` используется и в WinInet
  `ProxyOverride`, и в xray routing bypass.
- **`format=json` feed** в MVP не импортируется; только `format=subscription`.
- **Scheduler gate:** `auto_refetch_enabled` AND `subscription.auto_refetch_minutes>0`.
  `is_default` у service profile — только маркер сида; не делает профиль
  обязательным, не включает его автоматически и не запрещает удаление.

## Architecture

```
apps/desktop/src/
  pages/            Dashboard, Subscriptions, ServiceProfiles, Servers, Settings
  features/        .import-subscription, subscription-list, server-table, service-profile-editor, connection-panel
  stores/           app.store (UI), connection.store (runtime)
  lib/              invoke.ts (typed Tauri invoke + Zod parse)
packages/contracts/  общие типы и Zod-схемы команд/событий
packages/ui/         shadcn primitives
apps/desktop/src-tauri/src/
  main.rs
  commands/         subscription_*, service_profile_*, probe_*, connection_*, settings_*
  core/             ConfigSnapshot, XrayConfigBuilder, XraySupervisor
  subscription/     fetch.rs, decode.rs, upsert.rs
  probe/            delay.rs, service_check.rs, scheduler.rs
  storage/          schema.sql, repositories
  platform/         PlatformProxy (Windows impl), AntiDpiService (no-op в MVP)
  binaries/         xray[.exe], winws2.exe + WinDivert.* + lua/, nfqws2 (linux) — gitignored
```

Rust — единственный источник истины для сети/процессов. React вызывает типизированные
команды и подписывается на Tauri events (`probe-progress`, `connection-state`,
`subscription-refreshed`). События и payload описаны в `packages/contracts`.

### Immutable snapshot

`ConfigSnapshot { subscription_id, profile_id, proxy_port, generated_config_path,
created_at }` создаётся один раз и передаётся и в `XrayConfigBuilder`, и в
`XraySupervisor` (по образцу v2rayN `CoreConfigContext`). Повторный запуск =
новый snapshot; гонки между генерацией конфига и запуском исключены.

## Data model (SQLite)

- `subscriptions`: `id TEXT PK`, `name`, `url`, `enabled BOOL`, `status`
  (`active|inactive|error`), `auto_refetch_minutes INT` (0 = выкл),
  `last_fetched_at`, `last_error`, `user_agent`, `request_headers TEXT JSON`,
  `sort INT`, timestamps.
- `profiles`: `id TEXT PK`, `subscription_id FK`, `server_id TEXT UNIQUE(subscription_id,server_id)`,
  `link`, `remarks`, `protocol`, `address`, `port`, `raw_config TEXT JSON`,
  `present BOOL`, `latency_ms INT NULL`, `reachable BOOL`, `checked_at`,
  `sort_index INT`, timestamps. Результаты тестов здесь же как кэш, но отдельные
  таблицы ниже — источник для фильтрации.
- `service_profiles`: `id TEXT PK`, `name UNIQUE`, `test_urls TEXT JSON`,
  `is_enabled BOOL`, `is_default BOOL`, timestamps. Полное соответствие
  `../mglaroute-vpn-server/backend/src/online-services/online-service.entity.ts`.
  Сид по умолчанию: YouTube `https://www.youtube.com`, Discord `https://discord.com`,
  Google `https://www.google.com` (как на сервере).
- `profile_service_results`: `profile_id`, `service_profile_id`, `reachable BOOL`,
  `checked_at`; PK `(profile_id, service_profile_id)`.
- `settings` (singleton): `auto_refetch_enabled`, `refetch_interval_minutes`,
  `auto_test_enabled`, `test_interval_minutes`, `test_timeout_ms`,
  `auto_reconnect_enabled`, `reconnect_cooldown_seconds`, `ping_url`,
  `local_proxy_port`, `proxy_exclusions`, `system_proxy_enabled`.

## Tauri command surface (MVP)

```
subscription_add(url, name?) -> Subscription
subscription_list() -> Subscription[]
subscription_update(id, patch)
subscription_remove(id)
subscription_refresh(id) -> RefreshResult   # fetch + decode + upsert
service_profile_list/create/update/remove
settings_get/update
probe_run() -> ProbeResult                  # ручной цикл
connection_status() -> ConnectionState
connection_connect(profile_id?) -> ConnectionState
connection_disconnect() -> ConnectionState
```

`AntiDpiService` команды объявлены, но возвращают `Unsupported` в MVP.

## Flows

### 1. Импорт / refetch подписки

1. `reqwest` GET, таймаут 10 c, `proxy: none` (не зависеть от системного прокси).
2. Декод каскадом (как v2rayN `AddBatchServers`):
   - если тело base64 (standard/url-safe, normalize `-`/`_`, padding) → декодировать;
   - сплит по строкам, `trim`, `dedup` по точной строке;
   - по префиксу: `vmess://` (legacy base64 JSON `ps/add/port/id/aid/scy/net/host/path/tls/sni/alpn/fp`),
     `vless://`, `trojan://`, `ss://` (SIP002 + legacy). Всё остальное — count bad, skip.
   - hash-параметры vless/trojan (`security,sni,fp,pbk,sid,spx,flow,encryption,type,path,host,alpn…`)
     сохранить в `raw_config`.
3. Upsert по `(subscription_id, server_id)`, `server_id` = sha256(link). Поля скана
   (`latency_ms`, `reachable`, `checked_at`) сохранить. Отсутствующие в новом теле
   профили → `present=false` (скрыть, но не удалять).
4. Успех → `status=active`, `last_fetched_at=now`, `serverCount`. Ошибка →
   `status=error`, `last_error`, старые профили остаются, `last_fetched_at` не
   обновляется (повтор на следующем тике).

`subscription_refresh` — общий путь и для ручного, и для auto-refetch.
Auto-refetch: `tokio` periodic 1 мин, обновлять подписки с `enabled=true`,
`auto_refetch_minutes>0` и `now-last_fetched_at >= interval`.

### 2. Auto-test + сортировка

1. Отобрать `present=true` профили активных подписок (`enabled=true`).
2. Батчами (по 20) собрать один xray config: `log.loglevel=warning`, N `mixed`
   inbounds `127.0.0.1:<free port>` `noauth`, N outbounds (по одному на профиль),
   routing rule по inbound tag → outbound tag. `domainStrategy=AsIs`.
3. Запустить ядро, дождаться портов (socks5 handshake или TCP connect, 2 c).
4. Для каждого профиля: 2 GET ping URL через `proxy = http://127.0.0.1:<port>`
   (explicit, не ENV), `min positive ms`; timeout `test_timeout_ms`.
5. Для каждого активного `service_profile`: GET всех `test_urls` через тот же
   порт; `reachable = all 2xx`. Записать в `profile_service_results`.
6. Убить ядро, записать `latency_ms`/`reachable`/`checked_at`, sort:
   delay ASC, failed в конец; `sort_index = (i+1)*10`.
7. Событие `probe-progress` после каждого батча и `probe-finished` в конце.

### 3. Подключение / отключение

- `connection_connect(profile_id?)`: если id не задан — лучший сервер, прошедший
  все активные сервис-профили. Построить config snapshot (один `mixed` inbound на
  `local_proxy_port`, outbound из профиля, `direct` fallback), записать temp config
  в app data, spawn `xray run -c <path>` через `XraySupervisor`, дождаться порта.
- Windows: записать WinInet per-connection proxy + registry (`ProxyEnable=1`,
  `ProxyServer=127.0.0.1:<port>`, `ProxyOverride=<local>;...`), применить
  `INTERNET_OPTION_SETTINGS_CHANGED` + `REFRESH`; сохранить previous state в памяти.
- `connection_disconnect`: снять прокси (вернуть previous),kill child, clean temp config.
- Crash child → событие `connection-state=Faulted`; UI предлагает reconnect.
  Авто-restart ядра — вне MVP, но supervisor ловит exit и переводит состояние.

### 4. Авто-reconnect

После успешного цикла `probe_run`, если `auto_reconnect_enabled` и connected:
- выбрать лучшего кандидата (прошёл все активные сервис-профили, min delay);
- если кандидат != текущий и `now-last_switch >= reconnect_cooldown_seconds` →
  `connection_connect(candidate)`;
- если текущий более не проходит активные профили → переключить без cooldown.

## Анти-DPI (отложено, в сборке только бинарники)

- Build-time копирует в `binaries/`: Windows — `winws2.exe`, `cygwin1.dll`,
  `WinDivert.dll`, `WinDivert64.sys`, `lua/*.lua`; Linux — `nfqws2`, `lua/*.lua`.
  Все с sha256-проверкой; вместе копируются лицензии.
- В MVP `AntiDpiService::start/stop/status` возвращают `Unsupported`.
- Будущий этап: свой Rust supervisor, 3–5 курированных TLS-desync пресетов из
  Igareck catalogue, выбор физического адаптера и TCP-портов (не доменов),
  UAC/polkit helper, восстановление состояния при выходе. Запрещено обещать
  поддержку Android (нужен root).

## Tests (TDD до implementing)

- `decode`: vmess legacy base64, vless/trojan hash params, ss SIP002+legacy,
  whole-body base64 std+url-safe, dedup, bad lines counted.
- `upsert`: обновление существующего, `present=false` для исчезнувших, сохранение
  latency при повторном import.
- `service_check`: все 2xx → true; один non-2xx → false; пустой список → false;
  explicit proxy используется, ENV-прокси игнорируется.
- `delay`: 2 GET, min positive, timeout → null.
- `xray_config`: snapshot → config, N inbounds/outbounds, валидный JSON.
- `auto_reconnect`: cooldown, смена только при лучшем кандидате, принудительная
  смена если текущий не проходит фильтры.
- Windows proxy mock: set/unset/restore previous state.

## Risks / open items

- Свободный loopback-порт: гонка между выбором и bind xray → retry 1 раз.
- Время старта xray variadic → `XRAY_STARTUP_TIMEOUT_MS` (default 2000).
- WinDivert: DLL/sys version mismatch, AV-карантин, отсутствие сервиса драйвера —
  всё это проявится на этапе anti-DPI; в MVP не трогаем.
- Bundled xray для Linux: glibc/musl совместимость; проверять `xray -version` в CI.
- SQLite миграции: версионировать через `user_version`, без externals.

## What's left to builder

Вся реализация: Nx scaffolding, Tauri config, Rust modules, React pages, CI.
Решения, зафиксированные оператором: системный прокси в MVP; bundled бинарники;
анти-DPI вне MVP; требование прохождения всех URL активного профиля;
авто-reconnect после цикла с cooldown.
