# BUGS.md — находки ревью PR #68 (`feature/workflows-redesign`)

Дата: 2026-08-23. Базовая ветка: `develop`, HEAD: `ecf9198`.
Статус: все находки перепроверены по коду на диске (v2).

Валидация ветки зелёная: `cargo fmt --check` ✓, `clippy --workspace --all-targets --all-features -- -D warnings` → 0 warnings ✓,
`cargo test --workspace` → 178+7 passed ✓, Xvfb UI-тесты `--ignored` → 36/36 ✓.

---

## 1. [BLOCKER] Цикл ссылок: каждый `RepoDetailPane` бессмертен

**Симптом.** При переключении репо в сайдбаре (и при повторной загрузке после смены языка)
предыдущий detail-pane не освобождается: `selection.rs` снимает strong refs и вызывает
`deactivate()`, но поддерево панели висит само на себе. Каждый переключённый репо
навсегда утекает: все виджеты, модели и живые состояния.

**Почему утекают даже таймеры «не по расписанию»:** handler удерживает clone панели,
поэтому `lifecycle_token` (`Rc<()>`, `mod.rs:295`) никогда не достигает `strong_count == 1`,
и teardown в `Drop` (`mod.rs:143-148`) не срабатывает. `deactivate()`
(`mod.rs:332-335`, вызовы в `selection.rs:134/182/222`) останавливает таймеры явно,
но дерево виджетов и состояние утекают.

**Цепочка (все звенья — strong refs):**

1. `src/ui/detail_view/run_filters.rs:49-54` — `attach_filter_chip_handler`:
   обработчик `toggled` чип-кнопки захватывает `let pane = self.clone()`
   (целый `RepoDetailPane`);
2. `RepoDetailPane` (`src/ui/detail_view/mod.rs`) владеет деревом: поле
   `workflow_store` (`mod.rs:277`) и `root`/`toast_overlay` (`mod.rs:278-279`)
   сильно держат workflow-строки;
3. `src/ui/detail_view/helpers/workflows.rs:540` —
   `set_data(&expander, "actioneer-run-list", run_list.clone())`:
   gdata на expander'е workflow-строки держит `WorkflowRunListModel` сильно;
4. `src/ui/detail_view/helpers/runs/list.rs:220` — `set_detail_header` кладёт
   `DetailHeaderState` в `Rc<RefCell<Option<…>>>` (strong)
   (вызов из `workflows.rs:538`);
5. `src/ui/detail_view/header_state.rs:20` — `DetailHeaderState` сильно держит
   `filter_chips: FilterChips`;
6. `src/ui/detail_view/filter_controls.rs:5-12` — `FilterChips` сильно держит три
   `ToggleButton` (success/failed/running) → назад в их обработчик (п. 1).

Handler подключается в `RepoDetailPane::new` → `pane.connect_filter_chips()`
(`mod.rs:302`), т.е. в живом приложении цепь замкнута всегда.

**Правило, которое нарушено:** AGENTS.md — «Signal handlers must not hold a strong
reference to any ancestor of the widget they are attached to».

**Почему тесты не поймали.** В ветке два row-level release-теста:

- `run_row_is_released_when_dropped` (`src/ui/detail_view/helpers/runs/row.rs:458`) —
  проверяет строку RUN (`create_run_expander_row` со stub-`RunRowContext`);
  чипы/pane в нём вообще не участвуют;
- `workflow_row_is_released_when_dropped`
  (`src/ui/detail_view/helpers/workflows.rs:1443`) — проверяет workflow-строку и
  накрывает почти всю цепь: реальный `WorkflowRowContext` со стаб-
  `FilterControls::new()` (`workflows.rs:1399`) и реальным
  `DetailHeaderState::new(labels, controls.chips)` (→ модель держит header,
  header держит чипы, чипы лежат в строке), реальный
  `create_workflow_expander_row` (→ gdata → модель). Комментарий теста прямо называет
  этот класс цикла: «the run-list model holding a header that held the very buttons
  whose handler owns that model».

Что НЕ покрыто ни одним тестом — замыкающее звено: `toggled`-обработчик чипов,
захватывающий clone `RepoDetailPane` (п. 1). Он существует только после
`connect_filter_chips()`, а тот требует реальной панели — тесты панель не строят,
строка в них умирает чисто, и оба теста зелёные. **Pane-level release-теста нет.**

**Минимальное исправление.** `WorkflowRunListModel` использует `detail_header`
только для `latest_run(workflow_id)` (`list.rs:246`), `record_latest_run`
(`load.rs:155,238`) и обновления счётчиков. Заменить в модели
`detail_header: Rc<RefCell<Option<DetailHeaderState>>>` на data-only хэндл:

- общую карту `latest_runs: Rc<RefCell<HashMap<i64, WorkflowRun>>>`
  (уже существует и уже общая — `header_state.rs:21`);
- три count-`Label` (листовые виджеты без обработчиков — обратного ребра не создают);

и пересчитывать счётчики прямо в модели (логика `DetailHeaderState::refresh_counts`,
`header_state.rs:106-116`). Модель при этом НЕ должна держать `FilterChips`/кнопки —
иначе цикл вернётся. Учтите, что `FilterChips::set_counts`
(`filter_controls.rs:21-35`) кроме цифр обновляет ещё и tooltip'ы
(`describe_control`): либо tooltip-обновление оставить на стороне панели через ту же
общую карту, либо model обновляет только цифры.
Это рвёт цепь на сильном звене: model → header → chips → button → pane.

Альтернативы (не предпочтительны): захват «кусков» вместо `pane` в
`attach_filter_chip_handler` — `on_filter_chip_toggled` →
`refresh_visible_runs_with_filters` ходит по многим полям панели, «кусков» будет
почти весь pane; weak на `DetailHeaderState` невозможен — это Rust-структура,
не GObject.

**Требующийся тест.** Pane-level release-тест по образцу
`window_is_released_once_closed` (`src/ui/job_logs_window.rs:701`) с
`test_helpers::collect_widget_weaks`: собрать настоящий `RepoDetailPane`
(т.е. с `connect_filter_chips`), снять strong refs, прогнать main context и
увериться, что всё поддерево умерло. По AGENTS.md — сначала доказать,
что тест проваливается, вернув цикл.

---

## 2. [minor, не блокирует] In-flight fetch в `JobLogsWindow` пинит `Ctx` после закрытия окна

`src/ui/job_logs_window.rs` — `load_logs`: receiver-closure держит `Rc<Ctx>`
(а внутри — strongly `text_view`) до получения ответа. Если окно закрыто, пока
запрос в полёте, Ctx доживает до ответа, после чего `render_logs` вызывается на
destroyed-виджете. Краш-безопасность в порядке: `Ctx` сильно держит `text_view`,
поэтому объект не финализируется, а вызовы GTK на destroyed-объекте — no-op;
просто один лишний цикл до ответа.

Статус: штатный паттерн проекта (то же в `create_logs_button`,
`src/ui/detail_view/helpers/runs/actions.rs`, где closure дополнительно пинит саму
кнопку). Временный пин, не утечка. Чинить не обязательно; упомянуто для полноты.

---

## 3. [проверено, не баг] i18n

28 новых msgid добавлены в `po/actioneer.pot` и во все 11 переводимых runtime-каталогов
(ar, bn, de, es, fr, hi, nl, pt_BR, ru, ur, zh_Hans), `.mo` перегенерированы —
всё согласовано (проверено программно: ни одного нового msgid не недостаёт ни в одном
из 11 каталогов).

`po/en.po`, `po/it.po`, `po/ja.po` не обновлялись: корректно. В `LanguagePreference`
(`src/preferences.rs`) 13 вариантов — `System` + 12 языков (en + те же 11 переводимых);
it/ja в рантайне не выбираются, en — источник строк (gettext падает на msgid).
Всего в `po/` 14 каталогов: 12 runtime + it + ja.

Статус: действий не требуется.

---

## Итог

- Мержить **нельзя**, пока не закрыт пункт 1 (blocker): утекает целый detail-pane
  на каждое переключение репо; механизм (цикл model → header → chips → button → pane)
  задокументирован в комментарии самого теста `workflow_row_is_released_when_dropped`,
  но замыкающее звено на уровне панели тестами не покрыто.
- Пункт 2 — по желанию; пункт 3 — информация.
