# Memento: продуктовая стратегия и дорожная карта

Дата: 18 июля 2026 года

Горизонт: 12 месяцев
Основная аудитория решения: владелец продукта и команда Memento

## Executive Summary

- **Memento стоит развивать не как ещё один “AI meeting notes”, а как локальную долговременную память о рабочих разговорах.** Базовый контракт: надёжно записать, точно понять, сохранить проверяемые факты, связать их с прошлым и вернуть нужный контекст в момент следующего решения.
- **Главный приоритет — доверие к основанию, а не ширина функций.** Пока ломаются аудио, язык, имена, стабильность спикеров или заголовки, любая суммаризация, mind map и “инсайты” умножают недоверие. Первые два этапа roadmap посвящены этому основанию и упрощению основного сценария.
- **Эффект накопления можно получить без обучения большой модели на GPU.** Повторные встречи должны улучшать Memento через подтверждённые голосовые профили, словарь терминов, сущности и алиасы, связи людей/проектов/серий, retrieval по архиву и журнал принятых или отклонённых фактов. Это непараметрическая память: обновляются данные и небольшие индексы, а не веса LLM.
- **Новые сценарии нужно строить как “доменные представления” поверх общего ядра.** 1-to-1, интервью, standup и клиентская встреча должны использовать единые сущности `decision`, `commitment`, `question`, `risk`, `claim`, `topic` и evidence-ссылки, а не превращаться в четыре независимых мини-продукта.

Рекомендуемая позиция:

> **Memento — приватная память о разговорах, которая помнит людей, решения и незакрытые обязательства и всегда показывает, откуда это известно.**

## 1. Какое решение мы принимаем

Нужно перестать оптимизировать Memento вокруг экрана отдельной записи и списка способов сгенерировать текст. Продукт должен оптимизироваться вокруг повторяющегося цикла:

1. разговор записался без риска потери;
2. пользователь быстро получил пригодный транскрипт;
3. Memento выделил проверяемые решения, обещания, вопросы и темы;
4. неоднозначности были исправлены один раз;
5. исправление улучшило следующие встречи;
6. перед следующим разговором Memento вернул релевантный контекст;
7. любой вывод можно открыть в исходном таймкоде.

Формула ценности:

`ценность Memento = доверие к текущей записи × полезность памяти во времени × способность довести разговор до действия`.

Это мультипликативная система. Если аудио не проигрывается, имя спикера “плывёт” или вывод нельзя проверить, ценность последующих слоёв стремится к нулю.

## 2. Что показал аудит текущего продукта

### Сильная сторона уже построена, но почти не собрана в продукт

В репозитории уже есть значительная часть нужного технологического основания:

- локальный capture микрофона и системного звука, восстановление и экспорт;
- несколько ASR-путей и локальные модели;
- diarization, speaker profiles, voice samples и centroids;
- correction loop, terminology aliases и evidence;
- chunks, embeddings, entities, collections, search и RAG-chat;
- meeting-type suggestions, reconciliation и quality observations;
- специализированные workflow для standup, interview и 1-to-1.

[План долгосрочного learning loop](../AUTO_LISTENING_LEARNING_LOOP.md) уже формулирует правильные safety-инварианты: prediction не является training label, raw transcript не перезаписывается, исторические изменения предлагаются и могут быть отменены. Это не нужно выбрасывать — это и есть будущий moat Memento.

### Архитектура стала шире, чем основной пользовательский сценарий

В текущих migrations есть 73 уникальные таблицы. Аналитическая группировка показывает, что 45 из них относятся либо к cross-meeting learning, либо к отдельным доменным workflow. Это не “плохо” само по себе, но является сигналом: сложность уже переместилась из эксперимента в архитектуру до того, как основной сценарий стал простым и надёжным.

Дополнительные признаки:

- около 320 зарегистрированных Tauri-команд;
- отдельные большие панели для Standup, Interview, One-on-One и Learning Review;
- в основном meeting view пользователь видит технические решения: “модель”, “шаблон”, “улучшить”, “определить спикеров”, “имена”;
- search, collections и chat существуют как отдельные места, хотя пользователь воспринимает их как один вопрос: “найди нужный контекст”.

Эти числа — proxy архитектурной поверхности, а не оценка качества кода или количество видимых функций.

### Внутреннее измерение подтверждает неправильный порядок приоритетов

[Standup evaluation](../STANDUP_WORKFLOW_PLAN.md) фиксирует диагностический baseline: на небольшом корпусе forced-template extraction давал много неподтверждённых decisions/actions и очень высокую задержку. В документе уже сделан верный вывод: повышать recall и добавлять новые функции до precision-first gating нельзя.

Продуктовый вывод шире standup: пока Memento не умеет консервативно сказать “не уверен” и приложить источник, generic “Insights” и автоматические actions будут производить видимость пользы, но разрушать доверие.

## 3. Рынок: что стало стандартом, а где остаётся окно

### Категория уже сошлась к commodity-набору

- [Otter](https://otter.ai/chat) предлагает chat по всем встречам, генерацию follow-up, action items и summaries.
- [Fireflies](https://docs.fireflies.ai/askfred/use-cases) делает meeting и cross-meeting Q&A, decisions, action tracking и отраслевые prompts.
- [Read AI](https://www.read.ai/) объединяет meeting recaps, action items и cited search по встречам, письмам и сообщениям.
- [Fathom](https://fathom.video/for/hubspot) превращает записи в structured insights и синхронизирует их с CRM.

Иными словами, `transcript + summary + tasks + chat` уже не дифференциатор. Это минимальная комплектация категории.

### Продукт со скрина показывает формат выдачи, а не устойчивый moat

Набор вкладок “Саммари / Задачи / Инсайты / Стенограмма / Mindmap” очень похож на модель [NoteFlow](https://note-flow.fr/): одна запись превращается в несколько представлений. Это хорошая идея для быстрого просмотра, особенно mind map для лекции, brainstorm или исследования.

Но её не следует копировать как верхнеуровневую архитектуру Memento:

- mind map — про форму чтения, не про качество памяти;
- “инсайты” без чёткой семантики и evidence становятся мусорной категорией;
- пять независимо генерируемых вкладок повышают стоимость, latency и вероятность противоречий;
- после закрытия одной встречи их ценность почти не растёт.

Правильная интерпретация: summary, tasks, insights и mind map — это разные **проекции одного проверенного графа фактов**, а не четыре независимых AI-пайплайна.

### Наиболее сильные продукты продают не вкладки, а изменение поведения

[Granola](https://www.granola.ai/) строит опыт вокруг bot-free capture, подготовки до встречи, простых notes и памяти после неё. Team Folders позволяют [задавать вопросы сразу по группе встреч](https://www.granola.ai/updates/say-hello-to-team-folders). Это ближе к правильному направлению: ценность возникает между встречами.

При этом локальность тоже перестаёт быть уникальной сама по себе: [MacWhisper](https://goodsnooze.gumroad.com/l/macwhisper) делает on-device transcription и поиск, а [Krisp](https://krisp.ai/) сочетает bot-free capture, on-device English transcription и custom vocabulary. Следовательно, “мы локальные” — сильный trust-атрибут, но не достаточное продуктовое обещание.

### Окно для Memento

Ни один из рассмотренных паттернов не объединяет особенно хорошо четыре качества:

1. локальное хранение и контролируемую облачную обработку;
2. хорошую русскую и смешанную речь;
3. накапливаемую память людей, терминов, проектов и решений;
4. evidence-first ответы с явным статусом подтверждения.

Именно это сочетание следует сделать фокусом Memento.

## 4. Кому нужен первый жизнеспособный Memento

### Primary wedge

Индивидуальный knowledge worker, который проводит много повторяющихся и чувствительных разговоров:

- руководитель небольшой продуктовой или инженерной команды;
- founder/операционный лидер;
- product researcher;
- recruiter или interviewer при строгой приватности;
- консультант с повторяющимися клиентами.

Общий job-to-be-done:

> “Мне нельзя держать в голове десятки разговоров. Перед следующим обсуждением покажи, что мы решили, что обещали, что изменилось и где это было сказано — не заставляя меня заново организовывать архив.”

### Почему не начинать с “для всех команд”

Командный продукт сразу требует синхронизации, permission model, sharing semantics, администрирования и разрешения конфликтов. Это отложит проверку главной гипотезы. Сначала нужно доказать, что **личная база из 10–30 встреч даёт измеримый memory lift**. Затем локальная память может стать shared memory с явными границами доступа.

## 5. Core functionality и уровни продукта

### Уровень 0. Trust substrate — обязательное ядро

Это то, что нужно растить первым:

1. **Capture:** one-click/auto capture, импорт, recovery, корректное сохранение, playback и export.
2. **Speech truth:** точная RU/EN/mixed transcription, таймкоды, синхронизация с аудио, отсутствие silent data loss.
3. **People truth:** стабильные speaker IDs внутри записи, подтверждённые имена между встречами, Unknown как нормальный ответ.
4. **Terminology truth:** имена, аббревиатуры и доменные слова исправляются один раз и используются дальше.
5. **Evidence:** каждый AI-вывод ведёт к сегменту транскрипта и аудио.

Пока этот уровень не проходит release gates, расширять продукт нельзя.

### Уровень 1. Universal meeting memory — общий язык продукта

Вместо prose-only summary Memento хранит универсальные объекты:

| Объект | Что означает | Обязательные поля |
| --- | --- | --- |
| `decision` | явный выбор или согласованное направление | status, evidence, participants |
| `commitment` | обещанное действие | owner или Unknown, due или Not stated, evidence |
| `question` | незакрытый вопрос | status, evidence |
| `risk` | явно обсуждённый риск | impact, status, evidence |
| `claim` | утверждение участника, не обязательно факт | speaker, evidence, review state |
| `topic` | устойчивая тема разговора | aliases, meeting scope |
| `note` | пользовательская заметка | privacy scope, author |

У каждого объекта есть состояние `proposed → confirmed/rejected → superseded`, версия и source links. Summary, task list и mind map рендерятся из этих объектов.

### Уровень 2. Longitudinal memory — настоящий compounding value

Объекты связываются со scope:

- meeting;
- recurring series;
- person/relationship;
- project/client;
- workspace.

Появляются пользовательские ответы, которых не даёт обычный meeting summarizer:

- “Что изменилось со времени прошлого sync?”
- “Что я обещал Андрею и что ещё открыто?”
- “Какие решения по релизу были отменены и почему?”
- “Подготовь меня к следующему 1-to-1.”
- “Какие темы повторяются в интервью пользователей?”

### Уровень 3. Action surfaces — ценность в момент работы

- pre-meeting brief;
- Today/Inbox с items, требующими review;
- общий список подтверждённых commitments;
- Ask с цитатами и ограничением scope;
- person, project и series pages;
- controlled export/integrations.

### Уровень 4. Domain packs — специализация без распада продукта

| Pack | Специфическая ценность | Что остаётся общим |
| --- | --- | --- |
| Project / standup | changes, blockers, decisions, carry-forward | speakers, evidence, commitments, series |
| 1-to-1 | recurring topics, mutual commitments, private notes | people, timeline, evidence, privacy states |
| Interview | competency evidence, open questions, handoff | claims, evidence, review, sensitive scope |
| Research | themes across sessions, contradictions, quotes | entities, topics, citations, collections |
| Customer / sales | objections, needs, follow-up | people, commitments, project/client scope |

Pack должен определять extraction schema, renderer, privacy defaults и suggested questions. Он не должен создавать отдельную навигацию, отдельный summary engine и отдельную модель истины.

## 6. Как Memento становится лучше без GPU-обучения

### Основной механизм: непараметрическая персональная память

Большую модель не нужно дообучать на каждом компьютере. Улучшение можно получить обновлением небольших локальных структур:

| Сигнал | Что сохраняем | Как улучшается следующая встреча |
| --- | --- | --- |
| Исправление транскрипта | raw text, corrected text, canonical term, scope | vocabulary hint и post-correction для того же проекта/человека |
| Подтверждение спикера | embedding sample, quality, centroid, negative evidence | кандидат имени ранжируется лучше; слабый матч остаётся Unknown |
| Подтверждение имени/алиаса | canonical person + alias + evidence | разные формы имени перестают создавать новых людей |
| Принятое решение/обещание | structured object + evidence + status | pre-brief и follow-up получают только доверенные факты |
| Встреча добавлена в серию | reviewed relation + cadence | retrieval сначала ищет в релевантной серии |
| Ответ Ask принят/отклонён | query, retrieved evidence, feedback | улучшается ranking и набор suggested questions |
| Изменение summary | diff по универсальным объектам | персонализируется renderer, а не “переучивается истина” |

[RAG](https://papers.neurips.cc/paper/2020/hash/6b493230205f780e1bc26945df7481e5-Abstract.html) как раз разделяет параметры модели и обновляемую внешнюю память. Для Memento это особенно уместно: новые встречи индексируются сразу, могут иметь provenance и удаляются без “разучивания” весов модели.

Speaker embeddings также являются готовыми компактными представлениями голоса; diarization-системы на базе [ECAPA-TDNN](https://www.isca-archive.org/interspeech_2021/dawalatabad21_interspeech.html) используют такие embeddings для различения спикеров. В Memento нужно обновлять подтверждённые centroids и thresholds, а не обучать speaker model с нуля.

### Важное ограничение: больше записей не всегда означает лучше

Сырая база без review может ухудшить retrieval, размножить ложные имена и закрепить hallucinations. Поэтому влияние данных должно зависеть от trust state:

- `raw` — доступно для поиска, но не является фактом;
- `inferred` — гипотеза модели, видима с confidence;
- `confirmed` — может влиять на память и proactive views;
- `rejected` — negative evidence;
- `superseded` — исторически сохранено, но не считается текущим.

Чем больше база, тем лучше Memento **только если** растёт доля правильно нормализованных связей и подтверждённых evidence objects.

### Когда всё-таки рассматривать fine-tuning

Не по календарю и не ради “своей модели”. Только если одновременно выполнены условия:

1. stable task/schema и frozen evaluation set;
2. ошибка повторяется после vocabulary, retrieval, reranking и deterministic gating;
3. есть достаточно consented и независимо проверенных labels;
4. train/dev/test разделены по людям и сериям, а не по сегментам одной встречи;
5. adapter/fine-tune даёт измеримый выигрыш без ухудшения open-set и privacy guardrails;
6. можно откатить версию и удалить contribution конкретного пользователя.

До этого GPU выгоднее использовать для inference тяжёлого ASR или diarization, а не для пользовательского обучения.

## 7. Продуктовый редизайн

### Новая информационная архитектура

Главная навигация:

1. **Сегодня** — начать запись/импорт, следующая встреча, pending review, открытые commitments.
2. **Память** — meetings, people, projects/clients и recurring series; поиск встроен сюда.
3. **Спросить** — один Ask с явным scope и citations, а не отдельные search и chat.
4. **Действия** — только подтверждённые commitments и reminders.
5. **Настройки** — внизу, вне основного потока.

“Collections” остаются внутренним механизмом scope, но для пользователя называются Projects, Clients, Series или Topics по смыслу.

### Экран встречи

Вместо двух перегруженных панелей и двух тулбаров:

- стабильный header: пользовательское название, дата, участники, серия, processing state;
- вкладка **Итоги**: overview, decisions, commitments, open questions;
- вкладка **Транскрипт**: аудио, текст, speaker correction, terminology correction;
- вкладка **Контекст**: связанные прошлые встречи, people/project timeline, “что изменилось”;
- кнопка **Спросить об этой встрече**;
- overflow для export, open folder, delete и advanced reprocessing.

Model/provider/template не должны находиться в постоянном toolbar. По умолчанию Memento выбирает approved pipeline из настроек. “Обработать заново другим способом” находится в advanced action и показывает последствия.

### Где место templates

Пользователь выбирает не `interview_memory` или `client_sync`, а понятную цель:

- “Обычная встреча”;
- “Проектный sync”;
- “1-to-1”;
- “Интервью”;
- “Исследовательская беседа”.

После выбора система объясняет, **что будет извлечено**, а не показывает имя технического шаблона. Для большинства встреч Memento предлагает режим и продолжает с безопасным default без обязательного подтверждения перед каждым запуском.

### Mind map и Insights

Mind map стоит добавить позже как on-demand view для длинных концептуальных разговоров:

- строится из `topic` и evidence-linked objects;
- каждый node открывает исходный момент;
- пользователь может поправить структуру;
- она не считается источником истины;
- она не занимает постоянную вкладку, если тип встречи не предполагает её полезность.

“Insights” нужно заменить конкретными представлениями: “что изменилось”, “повторяющиеся риски”, “незакрытые вопросы”, “темы интервью”. Generic Insights Beta лучше не делать.

## 8. Дорожная карта на 12 месяцев

### Phase 0 — Trust reset (0–6 недель)

Цель: основной сценарий перестаёт ломать доверие.

Работы:

- завершить исправления playback/export, языка, стабильности speaker IDs/names и title ownership;
- единый processing state с понятной ошибкой и retry;
- regression corpus RU, EN и mixed-language;
- локальная телеметрия качества без содержимого встреч;
- устранить raw enum/English leakage в UI;
- убрать автоматическое переименование пользовательских встреч;
- определить golden path и запретить расширение core UI до прохождения gates.

Exit criteria:

- ≥99% успешных capture/save/playback на поддерживаемых happy paths в test matrix;
- ни одно пользовательское название не меняется без явного действия;
- speaker numbering стабилен после reload/reprocess;
- 100% core journey имеет RU/EN-localization;
- каждая ошибка обработки видима, сохраняет исходные данные и предлагает recovery.

### Phase 1 — One obvious product (6–12 недель)

Цель: новый пользователь получает результат без понимания моделей и шаблонов.

Работы:

- навигация “Сегодня / Память / Спросить / Действия”;
- новый meeting screen “Итоги / Транскрипт / Контекст”;
- прогрессивное раскрытие advanced settings;
- единый meeting mode selector с человеческими названиями;
- evidence links для summary records;
- объединить search и archive chat в один retrieval experience;
- обработка автоматически использует approved defaults.

Exit criteria:

- 5 из 5 новых пользователей без подсказки могут записать встречу, найти транскрипт, исправить имя и открыть источник решения;
- ≥80% обработанных встреч не требуют открытия model/template settings;
- core task completion и error recovery измеряются по hardware cohort;
- ни один primary action не дублируется в двух toolbar.

### Phase 2 — Compounding memory (3–5 месяцев)

Цель: пятая встреча с человеком или проектом объективно полезнее первой.

Работы:

- canonical `memory_object` + `memory_object_evidence` + version/status layer;
- безопасная миграция существующих standup/interview/1-to-1 records в общий read model;
- confirmed speaker identity, aliases и terminology loop;
- people, project и series pages;
- hybrid retrieval: metadata → FTS → embeddings → rerank;
- cited Ask и “что изменилось”;
- автоматический pre-meeting brief;
- review queue, где пользователь подтверждает только high-impact ambiguity.

Exit criteria:

- ≥95% displayed speaker-name precision на consented held-out corpus;
- ≥95% cited-answer support rate для проверяемых фактов;
- после пяти повторных встреч снижается correction rate для подтверждённых людей/терминов;
- memory lift против same-meeting-only baseline положителен без роста unsupported answers;
- удаление человека/проекта действительно удаляет производные embeddings и links.

### Phase 3 — From memory to action (5–8 месяцев)

Цель: память помогает завершать работу, а не только вспоминать.

Работы:

- global confirmed commitments inbox;
- carry-forward и superseded decisions;
- controlled reminders и calendar context;
- export/sync в выбранные Jira/Linear/Notion/CRM через явное подтверждение;
- два domain pack по реальному usage: вероятный приоритет `Project` и `1-to-1` либо `Research`;
- weekly “what changed / what remains open” digest.

Exit criteria:

- ≥90% принятых external actions не требуют исправления owner/type;
- zero silent outbound actions;
- пользователи возвращаются к Memento до следующей встречи, а не только после неё;
- каждый proactive item объясняет причину и источник.

### Phase 4 — Selective expansion (8–12 месяцев)

Цель: расширить distribution, не разрушив trust model.

Кандидаты:

- mobile/in-person capture;
- encrypted multi-device sync;
- shared project memory с permission inheritance;
- SDK/import API;
- optional enterprise connectors;
- mind map и другие evidence-linked projections;
- лёгкие adapters/fine-tuning только после прохождения evaluation gates.

Exit criteria задаются отдельно для выбранного направления. Не запускать все кандидаты параллельно.

## 9. Приоритетный портфель

### P0 — делать сейчас

1. Capture/recovery/playback.
2. Transcript quality, timestamps и mixed-language.
3. Stable speakers, names и terminology.
4. Evidence-backed universal summary.
5. Simple library + cited Ask.
6. Longitudinal memory и review states.

### P1 — строить сразу после устойчивого ядра

1. People/project/series pages.
2. Pre-meeting briefs.
3. Confirmed commitments and decision history.
4. Calendar/import/export.
5. Один или два domain pack, выбранных по usage.

### P2 — важные расширения

1. Controlled integrations.
2. Mobile capture.
3. Encrypted sync/team scopes.
4. Research and customer intelligence views.

### P3 — только после доказанной потребности

1. Mind map как постоянная вкладка.
2. Generic “Insights”.
3. Sentiment, charisma, engagement и speaking score.
4. Live coaching.
5. Пользовательский fine-tuning больших моделей.

## 10. Что нужно сознательно прекратить

- добавлять новый template на каждый вид встречи;
- создавать новый workflow panel и набор таблиц для каждой идеи;
- показывать model/provider как ежедневное продуктовое решение;
- автоматически переименовывать, перестраивать или backfill-ить пользовательскую истину;
- отправлять неподтверждённый action во внешнюю систему;
- считать длинный AI-текст признаком хорошего результата;
- оптимизировать recall до precision и citation validity;
- обучать модель на predictions самой модели;
- измерять людей sentiment/speaking-time/productivity score без строгой доказанной необходимости.

## 11. Метрики, которые действительно покажут прогресс

### North Star

**Trusted memory uses per active user per week** — количество случаев, когда пользователь открыл, принял, использовал или экспортировал факт из прошлой встречи с валидным source link.

Minutes recorded и summaries generated остаются input metrics, но не North Star.

### Core quality

- capture success, recovery success, audio availability;
- correction rate на 1, 5 и 10 встрече в повторяющейся серии;
- speaker-name precision и Unknown calibration;
- timestamp validity;
- supported/unsupported rate для decisions и commitments;
- retrieval citation precision и answer groundedness;
- p50/p95 processing latency по device/model cohort.

### Product value

- time to first trusted result;
- доля пользователей, которые вернулись **до** следующей встречи;
- pre-brief open/use rate;
- accepted commitments acted on;
- cross-meeting Ask success;
- memory lift: качество ответа с историей против текущей встречи без истории;
- week-4 retention после 5+ записей.

### Guardrails

- false speaker merge rate;
- silent title/data mutation rate — целевой максимум 0;
- unsupported external action rate — целевой максимум 0;
- deletion/unlearning completeness;
- cloud processing without explicit policy — целевой максимум 0.

## 12. Ближайшие решения на 30 дней

1. Утвердить позиционирование “private longitudinal meeting memory”.
2. Зафиксировать единую canonical model для evidence objects до новых workflow.
3. Сформировать regression corpus: 30–50 RU/EN/mixed встреч, включая повторяющиеся пары и проекты.
4. Завершить текущий bug-fix пакет как Phase 0, не смешивая его с redesign.
5. Нарисовать и протестировать кликабельный прототип трёх экранов: Today, Meeting, Ask.
6. Провести пять usability sessions на задачах, а не на визуальной привлекательности.
7. Ввести quality dashboard по capture, transcript, speakers, evidence и memory lift.
8. Выбрать один пилотный compounding loop: `speaker + terminology + pre-brief` для повторяющейся серии.

## 13. Открытые вопросы

- Какой wedge подтверждается реальным использованием: project leads, 1-to-1 managers, researchers или interviews?
- Какой уровень облачной обработки допустим по умолчанию и как показать его без перегрузки?
- Должна ли Memento хранить audio постоянно, по policy или только до подтверждения transcript?
- Какая единица организации естественнее для пользователя: project, person, series или client?
- Какие два вида external action дают наибольшую ценность без сложной team infrastructure?
- Нужен ли shared workspace до того, как личная memory loop докажет retention?

## 14. Допущения и ограничения исследования

- Это product/architecture audit, а не исследование поведения пользователей: аналитики usage funnel и retention в доступных источниках нет.
- Количественные признаки репозитория описывают архитектурную поверхность на 18 июля 2026 года и не являются оценкой усилий или качества реализации.
- Рыночное сравнение основано на публичных страницах продуктов; claims о качестве конкурентов не проверялись независимыми тестами.
- Рекомендуемые пороги roadmap — release targets. Их нужно откалибровать по реальному hardware и consented corpus.
- Приоритет domain packs нужно подтвердить интервью и usage, а не выбирать только по объёму уже написанного кода.
