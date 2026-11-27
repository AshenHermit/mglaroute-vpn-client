# mglaroute-vpn-client — Connect/Probe по образцу v2rayN (дизайн)

Дата: 2026-11-25. Статус: **accepted** (реализовано в Task 1–6; верификация в Task 7).

## Intent / why

Текущий MVP функционально собран, но пользователь observes три сломанных сценария:
`Run test` не даёт видимого эффекта, повторный клик даёт `Another operation is in
progress`, `Connect` ничего не делает, `Refresh` даёт безразличное `Could not refresh
subscription`. Причина не в сети, а в архитектуре: один глобальный `Semaphore(1)`
на все мутации, последовательный probe без per-server статуса и жёсткая связь
Connect с кэшем тестов.

Эталон — `../../v2rayN/v2rayN`: тесты и рабочее соединение живут независимо,
тесты идут параллельно (`Parallel.ForEachAsync`, `MixedConcurrencyCount >= 10`),
каждая строка показывает `Testing...` в той же ячейке, куда позже падает результат,
все ядра останавливаются в `finally` и связываются Windows Job Object с
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.

## Найденное (root causes, с точками в коде)

- `src-tauri/src/state.rs:20-46` — `MutationGuard` = `Semaphore::new(1)` на всё.
  `probe_run` (`commands/probe.rs:205-209`) держит его весь цикл; `connection_connect`
  (`commands/connection.rs:264-271`) получает `Busy`. Фоновые `refetch_loop` /
  `auto_test_loop` (`commands/mod.rs:382-426`) молча skip'ятся при занятом guard.
- `probe/cycle.rs:98-113,283-317` — батчи по 20 обрабатываются последовательно;
  `probe-progress` эмитится только после батча, при <20 серверов UI молчит до конца.
  Per-server статуса в контракте нет.
- `ConnectionPanel.tsx:41-43` всегда шлёт `profileId: null`; `connect_impl`
  (`connection.rs:21-32`) требует `best_eligible_profile`, то есть кэшированный
  положительный delay + `reachable` по **всем** активным сервис-профилям. Свежий
  импорт → `NotFound` → `Connection failed`.
- `core/supervisor.rs:66-112` — `ChildGuard` убивает только прямого ребёнка, без
  Job Object (TODO в коде). На момент исследования найден реальный осиротевший
  `xray.exe` (PID 29912, родитель мёртв, temp-config остался в `%TEMP%`).
- `SubscriptionList.tsx:69-77` — `catch` игнорирует код ошибки: `busy`, `fetch`,
  `storage`, `not_found` → одно сообщение.

## Goals

1. Connect работает по первому клику: явный выбор сервера строкой + рабочий fallback.
2. Тесты и подключение полностью независимы: тест не вызывает `busy` у Connect.
3. Тесты параллельные, с ограниченной конкурентностью; результат по каждому серверу.
4. В UI видно, что идёт тест: общий индикатор и список серверов в статусе `Testing...`.
5. Ни одного осиротевшего `xray`: Job Object + pid-сайдкар + уборка при старте.
6. `Stop test` отменяет цикл и гарантированно останавливает все temporary cores.
7. Осмысленные сообщения об ошибках подписки по коду ошибки.

## Non-goals / anti-goals

- Не трогаем anti-DPI, TUN, Linux/Android/iOS, Clash/sing-box full-config import.
- Не меняем семантику service-profile для **auto-reconnect** (по-прежнему требует
  прохождения всех активных профилей и cooldown). Меняем только ручной connect.
- Не добавляем новый Rust crate: `futures = "0.3"` уже есть; расширяем features
  `windows-sys`.
- Не логируем URL подписок, credentials и raw config — включая pid-сайдкары.

## Constraints

- Стек фиксирован: Tauri v2 + Rust, React 18 + Zustand, SQLite/rusqlite, tokio.
  MVP — Windows desktop; всё OS-зависимое — за `cfg(windows)` / trait.
- Контракты TS/Zod в `packages/contracts` — единственный источник payload'ов.
- `schema.sql` имеет `PRAGMA user_version = 1`; новое поле требует миграции до 2.
- Сборка без реального xray не проходит `tauri-build`; тесты используют `$env:TAURI_CONFIG`
  с пустыми `externalBin` и fake launcher, как описано в `AGENTS.md`.

## Approaches considered

| Подход | Суть | Плюс | Минус | Решение |
|---|---|---|---|---|
| A. Mixed | Один temporary xray **на сервер**, до `test_concurrency` одновременно; конфиг из одного outbound | Изоляция отказов: падение ядра не роняет весь цикл; точная копия v2rayN mixedtest | До 10 процессов, ~1 с прогрев на каждый | **выбран оператором** |
| B. Realping | Один xray на батч с N inbounds, параллель только в HTTP-проверках | Один процесс, меньше spawn/памяти | Crash ядра обнуляет батч; нужен пере-deadline port readiness | запасной |
| C.Минимальный патч | Разделить guard, disabled-кнопки, оставить последовательный probe | Мало кода | Не даёт параллелизма и per-row статуса | отклонён |

## Решения, зафиксированные с оператором

- Тесты выполняются **параллельно с работающим соединением**; рабочее ядро и
  system proxy тестами не затрагиваются.
- Выбран подход **A**.
- Строки серверов кликабельны; Connect использует выбранный сервер. При отсутствии
  выбора: лучший по кэшу, затем первый доступный. Ручной connect **не** требует
  прохождения активных service-профилей.
- Concurrency тестов — настройка `test_concurrency`, default 10 (как v2rayN
  `MixedConcurrencyCount`), диапазон 1..20.
- `Stop test` отменяет цикл; partial-результаты сохраняются.

## Design

### 1. Раздельные шлюзы (вместо одного `MutationGuard`)

В `state.rs` — три именованных guard'а, каждый `Semaphore::new(1)` + `try_acquire`:

| Guard | Владельцы | Код busy |
|---|---|---|
| `ConnectionGuard` | `connection_connect`, `connection_disconnect`, `apply_reconnect` | `connection_busy` |
| `ProbeGuard` | `probe_run`, `auto_test_loop` | `probe_busy` |
| `MutationGuard` | `subscription_*`, `settings_update`, `refetch_loop` | `mutation_busy` |
| `ConnectionGuard` (try_acquire) | `handle_exited_child` / `child_exit_watch` | нет (тик пропускается) |

`CommandError` получает варианты с этими кодами; `invoke.ts` расширяет whitelist.
Клиент показывает разные сообщения: Connect при активном Connect — «Connection
operation in progress», тест при активном тесте — кнопка уже disabled. Ни один
guard не удерживается через `await` другой области. `apply_reconnect` берёт
`ConnectionGuard` **внутри** probe-цикла; если connect уже идёт — пропускаем тик
(auto-reconnect never steals the busy slot).

### 2. Probe cycle, подход A

`probe/cycle.rs`:

- Убираем runtime-батчинг `PROBE_BATCH_SIZE` и `build_speedtest_config` из рабочего
  пути ( builders оставляем только под тесты, либо удаляем — на усмотрение builder).
- Один цикл = `futures::stream::iter(profiles).buffer_unordered(test_concurrency)`;
  общий `ProbeGuard` держится только снаружи; `watch::Receiver<bool>` — флаг отмены
  (`probe_cancel` command пишет `true`).
- На один профиль:
  1. `available_port()` — один свободный loopback port; проверка live child до wait.
  2. `build_probe_config(profile, port)` — `mixed` inbound `127.0.0.1:<port>` noauth,
     один outbound из профиля, финальный routing rule. **Без** `direct` fallback и
     `proxy_exclusions`: тест обязан идти только через прокси (иначе exclusion
     domains уйдут мимо проверки).
  3. temp config `%TEMP%\mglaroute-probe-{pid}-{nanos}.json` + sidecar `.pid`.
  4. `ChildGuard::spawn(&snapshot, &job)` — AssignProcessToJobObject сразу после spawn.
  5. `wait_for_port(port, XRAY_STARTUP_TIMEOUT_MS default 2000)` с early-exit check;
     при таймауте — только этот сервер помечается `failed`, остальные продолжают.
  6. `measure_delay` (2 GET, min positive) + `check_service_profile` для каждого
     активного сервис-профиля (URL внутри профиля — `join_all`), explicit proxy,
     timeout `test_timeout_ms`.
  7. `guard.stop()` в `finally`-эквиваленте (RAII Drop + явный `stop`), удалить config
     и `.pid`.
  8. Собрать `ProbeSample`; при любом локальном сбое sample = `latency_ms: None`,
     все `service_results.reachable = false`.
- Событие `probe-server` эмитится **до** spawn (`status: "testing"`) и **после**
  (`status: "done" | "failed"` с результатом). `probe-progress` — после каждого
  сервера, `probe-finished` — в конце с `cancelled: bool`.
- Отмена: флаг проверяется перед spawn, после `wait_for_port`, между GET; уже
  запущенные ядра останавливаются в `finally`; `probe-finished(cancelled: true)`
  и сохранение накопленных sample'ов.
- Semantics `probe_cancel` (уточнение автора): новый Tauri command без guard'ов,
  пишет `true` в `watch` флаг и возвращает `bool` — `true`, если отмена принята
  (цикл реально идёт), `false`, если активного цикла нет. `probe-cancelled`
  эмитится сразу при принятии отмены (UI переводит кнопку в «Canceling...»);
  `probe-finished` эмитится в конце в любом случае, с `cancelled: true` при
  отмене. `probe_run` завершается нормально с `ProbeResult { cancelled: true }`
  и накопленными sample'ами — отмена пользователя не является ошибкой.
- При отмене partial sample'ы сохраняются, но **глобальный `sort_all` не
  вызывается**: ordering пользователя остаётся стабильным и не появляются
  коллизии `sort_index` у непротестированных серверов. Полный цикл по-прежнему
  пересортировывает все present-профили как сейчас.

### 3. Контракты и события

`packages/contracts/src/schemas.ts`:

- `EVENTS.probeServer = "probe-server"`, `EVENTS.probeCancelled = "probe-cancelled"`.
- `ProbeServerEventSchema`: `{ profileId, status: "testing"|"done"|"failed",
  latencyMs: number|null, serviceResults: [{ serviceProfileId, reachable }] }`.
- `ProbeResultSchema` += `cancelled: z.boolean().default(false)`.
- `SettingsSchema` / `SettingsPatchSchema` += `testConcurrency: z.number().int()`.
- `CommandErrorCode` += `connection_busy | probe_busy | mutation_busy`.

### 4. Процессы: никаких осиротевших ядер

- `core/supervisor.rs`: Windows `JobObject` (`CreateJobObjectW`,
  `SetInformationJobObject` с `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
  `AssignProcessToJobObject`) создаётся один раз в `setup` и живёт в `AppStateInner`.
  `ChildGuard::spawn` и `XrayLauncher` назначают job для **каждого** ядра — и connection,
  и probe. Закрытие job'а при выходе убивает всех потомков.
- `windows-sys` получить features `Win32_System_JobObjects`, `Win32_System_Threading`.
- Все дочерние процессы GUI-приложения создаются с `CREATE_NO_WINDOW`
  (`0x08000000`) через `core::supervisor::apply_hidden_console`: ядра xray и
  хелперы реапера `taskkill` / `tasklist` / `powershell`. Без флага Windows
  выделяет новую видимую консоль каждому консольному ребёнку — при
  `test_concurrency = 10` это даёт пачку окон на каждый цикл проверок.
  stdout/stderr ядра остаются `Stdio::null()`.
- pid-сайдкар рядом с каждым temp config. При старте `bootstrap()` сканирует
  `mglaroute-probe-*.pid` и `xray-*.pid`, проверяет, что PID всё ещё `xray.exe`
  (`QueryFullProcessImageNameW`) и в command line встречается путь конфига, затем
  `taskkill /PID <pid> /T /F`, удаляет config + `.pid`. Никаких секретов в логах.
- `ChildGuard::Drop` остаётся backstop; `stop()` собирает оба результата kill/wait.
- На non-Windows — `CommandExt::process_group(0)` + tree-kill как отдельный этап;
  в MVP Windows-only, это документированный follow-up, не блокер.

### 5. Connect и выбор сервера

- `ServerTable`: клик по строке → `selectedProfileId` в session-only Zustand store,
  визуальная метка выбранного. Передача `profileId` в `connection_connect`.
- `connect_impl`:
  - `Some(id)` → поиск среди `list_present_enabled`, **без** service-gating.
  - `None` → `best_eligible_profile`, иначе первый `present=true` **из
    `list_present_enabled()`** по `sort_index` (сервер из отключённой подписки не
    выбирается), иначе `NotFound`.
- `ConnectionPanel`: Connect разблокирован при `disconnected`/`faulted` **независимо**
  от теста. Сообщение «Another operation is in progress» заменено на адресное:
  Connect ↔ `connection_busy`, тест ↔ `probe_busy` (кнопка disabled), подписка ↔
  `mutation_busy`.
- Auto-reconnect остаётся в `apply_reconnect`: только прошедшие все активные
  сервис-профили, cooldown, снятие соединения если текущий перестал проходить.
  Он берёт `ConnectionGuard`, при `busy` тик пропускается.
- Ручной `probe_run` **не вызывает** `apply_reconnect`: по explicit-решению
  оператора рабочее соединение не связано с тестами. `run_impl` получает флаг
  `apply_reconnect`, который выставляет только `auto_test_loop`; ручной вызов
  передаёт `false`.

### 6. Подписки

- `refresh_impl` сохраняет текущую логику (fetch 10 s, `no_proxy`, best-effort decode,
  upsert, `status=error` при неудаче). Меняется только UI-маппинг кодов:
  `mutation_busy` → «Another operation is in progress. Try again shortly.»,
  `fetch` → «Could not reach subscription URL», `not_found` → «Subscription not found»,
  `storage` → «Local storage error», иначе «Could not refresh subscription».
- Auto-refetch по-прежнему skip'ится при занятом `MutationGuard`; новые guard'ы на
  него не влияют.

### 7. UI

- `app.store.ts`: `probeTesting: boolean`, `probeTestingIds: string[]`,
  `probeCancelled: boolean`; обработчики `probe-server` (добавить/убрать id),
  `probe-progress`, `probe-finished`, `probeCancelled`.
- `Servers.tsx`: кнопка `Run test` → `Testing...` + disabled во время цикла,
  рядом `Stop test`; счётчик `tested / total` виден всегда во время теста;
  `probe_busy` больше не показывает «Another operation...».
- `ServerTable.tsx`: принимает `testingIds`, `selectedId`, `onSelect`; строка в
  тесте показывает `Testing...` в колонке State и `…` в Delay, подсвечивается.
- `ConnectionPanel.tsx`: убрать связанность с probe; оставить `connection_busy`
  и timeout/guard locally.
- `SettingsForm.tsx`: новое числовое поле `testConcurrency` (1..20).
- `Dashboard.tsx`: показывать статус теста по `probeTesting` (общий индикатор).

### 8. Settings / storage

- `storage/schema.sql`: `test_concurrency INTEGER NOT NULL DEFAULT 10`;
  `PRAGMA user_version = 2` + миграция `ALTER TABLE settings ADD COLUMN` при
  `user_version = 1` (без externals, как зафиксировано в основном дизайне).
- `storage/models.rs`, `repos.rs`, `commands/settings.rs` — новое поле везде,
  clamp `1..=20` в `settings_update`.
- `SettingsPayload` и `SettingsPatch` — новое поле.

## Tests (TDD до implementing)

Rust:
- независимость guard'ов: `probe_busy` не блокирует `ConnectionGuard`, и наоборот;
- concurrency cap: FakeTransport считает максимум одновременно запущенных core;
- изоляция: один сбойшийся профиль не обнуляет остальные samples;
- события: `testing` → `done`/`failed` в правильном порядке, `probe-finished(cancelled)`;
- `connect(Some(id))` проходит без service-results; `connect(None)` → best → first → NotFound;
- job assignment и startup reaping на фейковом pid/конфиге (cfg(windows)).
TS/Vitest:
- `Servers`: disabled Run + visible Stop + progress, per-row `Testing...`;
- `ServerTable`: click select, selected highlight;
- `ConnectionPanel`: Connect доступен во время теста, `connection_busy` адресно;
- `SubscriptionList`: разные сообщения по кодам;
- `SettingsForm`: `testConcurrency` в форме и валидации.

## Risks / open items

- 10 одновременных xray — проверить на реальной машине; при давлении памяти
  оператор может снизить default или перейти на подход B.
- Windows Job Object требует прав на `AssignProcessToJobObject` — уже работает в
  v2rayN без admin, но проверить на целевой сборке.
- pid-сайдкар при аварийном выходе может указывать на переиспользованный PID —
  проверка image name + command line обязательна; при сомнении не убивать.
- Старый `build_speedtest_config`/батчинг удаляются из runtime; убедиться, что
  unit-тесты на них либо переписаны, либо перенесены.

## Уточнения после builder-check (решения автора)

Независимый ревьюер задал 5 вопросов. Все сведены к уже зафиксированным
решениям оператора или к деталям, которые builder вправе решить сам:

1. **Ручной `Run test` не вызывает reconnect** — следует из «рабочее соединение
   не должно быть связано с тестами». Разделение `run_impl(apply_reconnect)`
   описано в §5.
2. **`probe_cancel` / `probe-cancelled`** — семантика описана в §2; это новые
   command + event в контракте, отмена не считается ошибкой.
3. **Crash-watcher** — использует `ConnectionGuard` через `try_acquire`, тик при
   busy пропускается (§1). Probe не трогает `runtime.child`, поэтому с ProbeGuard
   он не пересекается.
4. **Fallback `connect(None)`** — только включённые подписки (`list_present_enabled`),
   иначе свежий импорт из отключённой подписки мог бы стать рабочим соединением (§5).
5. **Partial results при Stop** — сохраняются без глобального re-sort, чтобы не
   ломать порядок непротестированных серверов (§2).

## What's left to builder

После одобрения: написать план (`writing-plans`), затем реализовать по задачам с TDD,
обновить `AGENTS.md` (worktree), `docs/e2e-checklist.md` и contracts-тесты.
Все решения выше зафиксированы оператором; детали, помеченные «на усмотрение
builder», builder решает сам.
